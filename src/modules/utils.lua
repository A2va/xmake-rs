
-- Copyright (c) 2024 A2va

-- Permission is hereby granted, free of charge, to any
-- person obtaining a copy of this software and associated
-- documentation files (the "Software"), to deal in the
-- Software without restriction, including without
-- limitation the rights to use, copy, modify, merge,
-- publish, distribute, sublicense, and/or sell copies of
-- the Software, and to permit persons to whom the Software
-- is furnished to do so, subject to the following
-- conditions:

-- The above copyright notice and this permission notice
-- shall be included in all copies or substantial portions
-- of the Software.

-- THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF
-- ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED
-- TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A
-- PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT
-- SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
-- CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
-- OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR
-- IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
-- DEALINGS IN THE SOFTWARE.

import("core.base.graph")
import("core.base.hashset")
import("core.cache.memcache")
import("core.project.project")

import("rules.c++.modules.modules_support.stl_headers", {rootdir = os.programdir()})

function _hashset_join(self, ...)
    for _, h in ipairs({...}) do
        for v, _ in pairs(h:data()) do
            self:insert(v)
        end
    end
    return self
end

-- source: https://github.com/xmake-io/xmake/blob/dev/xmake/rules/c%2B%2B/modules/modules_support/compiler_support.lua
function _compiler_support(target)
    local memcache = memcache.cache("compiler_support")
    local cachekey = tostring(target)
    local compiler_support = memcache:get2("compiler_support", cachekey)
    if compiler_support == nil then
        local rootdir = path.join(os.programdir(), "rules", "c++", "modules", "modules_support")
        if target:has_tool("cxx", "clang", "clangxx") then
            compiler_support = import("clang.compiler_support", {anonymous = true, rootdir = rootdir})
        elseif target:has_tool("cxx", "gcc", "gxx") then
            compiler_support = import("gcc.compiler_support", {anonymous = true, rootdir = rootdir})
        elseif target:has_tool("cxx", "cl") then
            compiler_support = import("msvc.compiler_support", {anonymous = true, rootdir = rootdir})
        else
            local _, toolname = target:tool("cxx")
            raise("compiler(%s): does not support c++ module!", toolname)
        end
        memcache:set2("compiler_support", cachekey, compiler_support)
    end
    return compiler_support
end

-- return the available targets
-- the target is available under the following conditions:
-- the kind is either shared or static
-- the target has no deps or it's the last target in the deps chain
-- opt.targets: list of predifined targets
function _get_available_targets(opt)
    local opt = opt or {}
    local gh = graph.new(true)
    local set = hashset.new()

    local map = function(index, target)
        return project.target(target)
    end

    local targets = opt.targets and table.imap(opt.targets, map) or table.values(project.targets())
    assert(#targets > 0, "some targets are not found!")

    local memcache = memcache.cache("utils.get_available_targets")
    local cachekey = get_cache_key(targets)

    local cache = memcache:get2("utils.get_available_targets", cachekey)
    if cache then
        return cache.targets, cache.targetsname
    end

    for _, target in pairs(targets) do
        if not target:is_shared() and not target:is_static() then
            goto continue
        end

        local deps = target:get("deps")
        for _, dep in ipairs(deps) do
            gh:add_edge(target:name(), dep)
        end
        if not deps then
            set:insert(target:name())
        end

        ::continue::
    end

    local parents = hashset.new()
    local children = hashset.new()

    for _, edge in ipairs(graph:edges()) do
        parents:insert(edge:from())
        children:insert(edge:to())  
    end

    for _, child in children:keys() do
        set:remove(child)   
    end

    local targets = {}
    local targetsname = {}

    local result = _hashset_join(set, parents)
    for _, target in result:orderkeys() do
        table.insert(targetsname, target)
        table.insert(targets, project.target(target))
    end

    memcache:set("utils.get_available_targets", cachekey, {targets = targets, targetsname = targetsname})
    return targets, targetsname
end

-- get the targets
function get_targets()
    local list = _g.targets_list
    if list == nil then
        local values = os.getenv("XMAKERS_TARGETS") or nil
        if values then
            values = table.wrap(string.split(values, ";"))
        end
        local targets, targetsname = _get_available_targets({targets = values})
        list = {targets, targetsname}
        _g.targets_list = list
    end

    return list[1], list[2]
end

-- get a cache key for the given targets
function get_cache_key(targets)
    local targets = targets or get_targets()

    local key = {}
    for _, target in ipairs(targets) do
        table.insert(key, target:name())
    end
    return table.concat(key, "-")
end

-- retrieves a value from the specified target, using the given name and scope.
-- unpack the multiple return values into a single table.
function get_from_target(target, name, scope)
    local result, _ = target:get_from(name, scope)
    result = result or {}
    result = table.join(table.unpack(result))
    return table.wrap(result)
end

--- check if the given target uses the C++ standard library (STL) based on the provided include directories.
---  opt.strict:  if true, the include directory must exactly match the STL include directory
function is_stl_used(target, includes, opt)
    opt = opt or {}
    local stl_includedirs = _compiler_support(target).toolchain_includedirs(target)
    local std_used = false

    for _, include in ipairs(includes) do
        for _, stl_includedir in ipairs(stl_includedirs) do
            local file = path.relative(include, stl_includedir)
            
            local includedirs_check = opt.strict and include:startswith(stl_includedir) or true
            if includedirs_check and stl_headers.is_stl_header(file) then
                std_used = true
            end
        end

        if std_used then
            break
        end
    end

    return std_used
end