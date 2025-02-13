-- Licensed under the Apache License, Version 2.0 (the "License");
-- you may not use this file except in compliance with the License.
-- You may obtain a copy of the License at
--
--     http://www.apache.org/licenses/LICENSE-2.0
--
-- Unless required by applicable law or agreed to in writing, software
-- distributed under the License is distributed on an "AS IS" BASIS,
-- WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
-- See the License for the specific language governing permissions and
-- limitations under the License.
--
-- Copyright (C) 2015-present, TBOOX Open Source Group.
--

import("core.base.graph")
import("core.base.hashset")
import("core.project.config")
import("core.platform.platform")

-- source: https://github.com/xmake-io/xmake/blob/master/xmake/core/tool/builder.lua

-- builder: get the extra configuration from value
function _extraconf(extras, value)
    local extra = extras
    if extra then
        if type(value) == "table" then
            extra = extra[table.concat(value, "_")]
        else
            extra = extra[value]
        end
    end
    return extra
end

-- builder: add items from config
function _add_items_from_config(items, name, opt)
    local values = config.get(name)
    if values and name:endswith("dirs") then
        values = path.splitenv(values)
    end
    if values then
        table.insert(items, {
            name = name,
            values = table.wrap(values),
            check = opt.check,
            multival = opt.multival,
            mapper = opt.mapper})
    end
end

-- builder: add items from toolchain
function _add_items_from_toolchain(items, name, opt)
    local values
    local target = opt.target
    if target and target:type() == "target" then
        values = target:toolconfig(name)
    else
        values = platform.toolconfig(name)
    end
    if values then
        table.insert(items, {
            name = name,
            values = table.wrap(values),
            check = opt.check,
            multival = opt.multival,
            mapper = opt.mapper})
    end
end

-- builder: add items from option
function _add_items_from_option(items, name, opt)
    local values
    local target = opt.target
    if target then
        values = target:get(name)
    end
    if values then
        table.insert(items, {
            name = name,
            values = table.wrap(values),
            check = opt.check,
            multival = opt.multival,
            mapper = opt.mapper})
    end
end

-- builder: add items from target
function _add_items_from_target(items, name, opt)
    local target = opt.target
    if target then
        local result, sources = target:get_from(name, "*")
        if result then
            for idx, values in ipairs(result) do
                local source = sources[idx]
                local extras = target:extraconf_from(name, source)
                values = table.wrap(values)
                if values and #values > 0 then
                    table.insert(items, {
                        name = name,
                        values = values,
                        extras = extras,
                        check = opt.check,
                        multival = opt.multival,
                        mapper = opt.mapper})
                end
            end
        end
    end
end

-- builder: sort links of items
function _sort_links_of_items(items, opt)
    opt = opt or {}
    local sortlinks = false
    local makegroups = false
    local linkorders = table.wrap(opt.linkorders)
    if #linkorders > 0 then
        sortlinks = true
    end
    local linkgroups = table.wrap(opt.linkgroups)
    local linkgroups_set = hashset.new()
    if #linkgroups > 0 then
        makegroups = true
        for _, linkgroup in ipairs(linkgroups) do
            for _, link in ipairs(linkgroup) do
                linkgroups_set:insert(link)
            end
        end
    end

    -- get all links
    local links = {}
    local linkgroups_map = {}
    local extras_map = {}
    local link_mapper
    local framework_mapper
    local linkgroup_mapper
    if sortlinks or makegroups then
        local linkitems = {}
        table.remove_if(items, function (_, item)
            local name = item.name
            local removed = false
            if name == "links" or name == "syslinks" then
                link_mapper = item.mapper
                removed = true
                table.insert(linkitems, item)
            elseif name == "frameworks" then
                framework_mapper = item.mapper
                removed = true
                table.insert(linkitems, item)
            elseif name == "linkgroups" then
                linkgroup_mapper = item.mapper
                removed = true
                table.insert(linkitems, item)
            end
            return removed
        end)

        -- @note table.remove_if will traverse backwards,
        -- we need to fix the initial link order first to make sure the syslinks are in the correct order
        linkitems = table.reverse(linkitems)
        for _, item in ipairs(linkitems) do
            local name = item.name
            for _, value in ipairs(item.values) do
                if name == "links" or name == "syslinks" then
                    if not linkgroups_set:has(value) then
                        table.insert(links, value)
                    end
                elseif name == "frameworks" then
                    table.insert(links, "framework::" .. value)
                elseif name == "linkgroups" then
                    local extras = item.extras
                    local extra = _extraconf(extras, value)
                    local key = extra and extra.name or tostring(value)
                    table.insert(links, "linkgroup::" .. key)
                    linkgroups_map[key] = value
                    extras_map[key] = extras
                end
            end
        end

        links = table.reverse_unique(links)
    end

    -- sort sublinks
    if sortlinks then
        local gh = graph.new(true)
        local from
        local original_deps = {}
        for _, link in ipairs(links) do
            local to = link
            if from and to then
                original_deps[from] = to
            end
            from = to
        end
        -- we need remove cycle in original links
        -- e.g.
        --
        -- case1:
        -- original_deps: a -> b -> c -> d -> e
        -- new deps: e -> b
        -- graph: a -> b -> c -> d    e  (remove d -> e, add d -> nil)
        --            /|\             |
        --              --------------
        --
        -- case2:
        -- original_deps: a -> b -> c -> d -> e
        -- new deps: b -> a
        --
        --         ---------
        --        |        \|/
        -- graph: a    b -> c -> d -> e  (remove a -> b, add a -> c)
        --       /|\   |
        --         ----
        --
        local function remove_cycle_in_original_deps(f, t)
            local k
            local v = t
            while v ~= f do
                k = v
                v = original_deps[v]
                if v == nil then
                    break
                end
            end
            if v == f and k ~= nil then
                -- break the original from node, link to next node
                -- e.g.
                -- case1: d -x-> e, d -> nil, k: d, f: e
                -- case2: a -x-> b, a -> c, k: a, f: b
                original_deps[k] = original_deps[f]
            end
        end
        local links_set = hashset.from(links)
        for _, linkorder in ipairs(linkorders) do
            local from
            for _, link in ipairs(linkorder) do
                if links_set:has(link) then
                    local to = link
                    if from and to then
                        remove_cycle_in_original_deps(from, to)
                        gh:add_edge(from, to)
                    end
                    from = to
                end
            end
        end
        for k, v in pairs(original_deps) do
            gh:add_edge(k, v)
        end
        if not gh:empty() then
            local cycle = gh:find_cycle()
            if cycle then
                utils.warning("cycle links found in add_linkorders(): %s", table.concat(cycle, " -> "))
            end
            links = gh:topological_sort()
        end
    end

    -- re-generate links to items list
    if sortlinks or makegroups then
        for _, link in ipairs(links) do
            if link:startswith("framework::") then
                link = link:sub(12)
                table.insert(items, {name = "frameworks", values = table.wrap(link), check = false, multival = false, mapper = framework_mapper})
            elseif link:startswith("linkgroup::") then
                local key = link:sub(12)
                local values = linkgroups_map[key]
                local extras = extras_map[key]
                table.insert(items, {name = "linkgroups", values = table.wrap(values), extras = extras, check = false, multival = false, mapper = linkgroup_mapper})
            else
                table.insert(items, {name = "links", values = table.wrap(link), check = false, multival = false, mapper = link_mapper})
            end
        end
    end
end

-- get the links in the correct order
function orderlinks(target)
    assert(target:is_binary(), "linkorders() requires a binary target")
    local linkorders = {}
    local linkgroups = {}

    local values = target:get_from("linkorders", "*")
    if values then
        for _, value in ipairs(values) do
            table.join2(linkorders, value)
        end
    end
    values = target:get_from("linkgroups", "*")
    if values then
        for _, value in ipairs(values) do
            table.join2(linkgroups, value)
        end
    end

    local items = {}

    _add_items_from_target(items, "linkgroups", {target = target})

    _add_items_from_config(items, "links", {target = target})
    _add_items_from_target(items, "links", {target = target})
    _add_items_from_option(items, "links", {target = target})
    _add_items_from_toolchain(items, "links", {target = target})

    _add_items_from_config(items, "frameworks", {target = target})
    _add_items_from_target(items, "frameworks", {target = target})
    _add_items_from_option(items, "frameworks", {target = target})
    _add_items_from_toolchain(items, "frameworks", {target = target})

    _add_items_from_config(items, "syslinks", {target = target})
    _add_items_from_target(items, "syslinks", {target = target})
    _add_items_from_option(items, "syslinks", {target = target})
    _add_items_from_toolchain(items, "syslinks", {target = target})


    if #linkorders > 0 or #linkgroups > 0 then
        _sort_links_of_items(items, {linkorders = linkorders, linkgroups = linkgroups})
    end
    return items
end 