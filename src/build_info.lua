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

import("core.cache.memcache")
import("core.cache.localcache")

import("core.project.config")
import("core.project.depend")
import("core.project.project")

import("core.base.task")
import("core.base.hashset")

import("async.runjobs")
import("lib.detect.find_library")

import("modules.utils")
import("modules.builder")


-- return true if the cache needs to be invalidated
function _cache_invalidation()
    local changed = false

    local config_hash = hash.sha256(config.filepath())
    changed = config_hash ~= localcache.get("xmake_rs", "config_hash")
    if changed then
        localcache.set("xmake_rs", "config_hash", config_hash)
        localcache.save("xmake_rs")
        return changed
    end

    local mtimes = project.mtimes()
    local mtimes_prev = localcache.get("config", "mtimes")
    if mtimes_prev then
        for file, mtime in pairs(mtimes) do
            -- modified? reconfig and rebuild it
            local mtime_prev = mtimes_prev[file]
            if not mtime_prev or mtime > mtime_prev then
                changed = true
                break
            end
        end
    end
    
    return changed
end

function _include_scanner(target)
    local memcache = memcache.cache("include_scanner")
    local cachekey = tostring(target)
    local scanner = memcache:get2("include_scanner", cachekey)
    if scanner == nil then
        if target:has_tool("cxx", "clang", "clangxx") or target:has_tool("cxx", "gcc", "gxx") then
            scanner = import("modules.include_scanner.clang_gcc", {anonymous = true})
        elseif target:has_tool("cxx", "cl") then
            scanner = import("modules.include_scanner.msvc", {anonymous = true})
        else
            local _, toolname = target:tool("cxx")
            raise("compiler(%s): is not supported!", toolname)
        end
        memcache:set2("include_scanner", cachekey, scanner)
    end
    return scanner
end

function _print_infos(infos)
    
    print("__xmakers_start__")
    for k, v in table.orderpairs(infos) do
        -- links are handled differently
        if k == "links" then
            v = table.imap(v, function(index, v)
                return format("%s/%s", v.name, v.kind)
            end)
        end

        if type(v) == "table" then
            v  = table.concat(v, "|")
        end

        print(k .. ":" .. tostring(v))
    end
    print("__xmakers_stop__")
end

function _get_linkdirs(target, opt)
    local opt = opt or {}

    local linkdirs = table.imap(utils.get_from_target(target, "linkdirs", "*"), function(index, linkdir)
        if not path.is_absolute(linkdir) then
            linkdir = path.absolute(linkdir, os.projectdir())
        end
        return linkdir
    end)
    linkdirs = hashset.from(linkdirs)
    local envs = target:pkgenvs()

    local values = ""
    if envs then
        values = is_plat("windows") and envs.PATH or envs.LD_LIBRARY_PATH
        if is_plat("macosx") then
            values = envs.DYLD_LIBRARY_PATH
        end
    end

    for _, v in ipairs(path.splitenv(values)) do
        if not linkdirs:has(v) then
            linkdirs:insert(v)
        end
    end

    if is_plat("windows") and opt.runenvs then
        for _, toolchain in ipairs(target:toolchains()) do
            local runenvs = toolchain:runenvs()
            if runenvs and runenvs.PATH then
                for _, env in ipairs(path.splitenv(runenvs.PATH)) do
                    linkdirs:insert(env)
                end
            end
        end
    end

    return linkdirs:to_array()
end

function _find_kind(link, linkdirs)
    -- in find_library of the xmake codebase, shared comes before static in the kind table.
    -- but on windows there is a problem because .lib (static librairies) files can appear in a shared package/target
    -- thus guessing wrongly the link kind, to prevent that we must detect shared library before static one. 
    local lib = find_library(link, linkdirs, {kind = {"shared", "static"}, plat = config.plat()})
    if not lib then
        return "unknown"
    end
    return lib.kind
end

function _get_links(target)
    local linkdirs = _get_linkdirs(target)
    
    local syslinks = hashset.from(utils.get_from_target(target, "syslinks", "*"))
    local frameworks = hashset.from(utils.get_from_target(target, "frameworks", "*"))

    local have_groups_order = (target:get_from("linkgroups", "*") ~= nil) or (target:get_from("linkorders", "*") ~= nil)

    -- a target can have a different file name from the target name (set_basename)
    -- so map the target name to the generated file name
    local filename_map = {}
    for _, target in pairs(project.targets()) do
        if target:is_static() or target:is_shared() then
            filename_map[target:linkname()] = utils.get_namespace_target(target)
        end
    end
    local is_target = function(link)
        return filename_map[link] ~= nil
    end

    local items = builder.orderlinks(target)
    local links = {}

    for _, item in ipairs(items) do 
        local values = (type(item.values[1]) == "table") and table.unpack(item.values) or item.values
        
        local is_syslinks = item.name == "syslinks"
        local is_frameworks = item.name == "frameworks"

        if not (have_groups_order or is_syslinks or is_frameworks) and not is_target(values[1]) then
            goto continue
        end

        for _, value in ipairs(values) do
            local kind = is_syslinks and "syslinks" or nil
            kind = is_frameworks and "frameworks" or kind

            if item.name == "linkgroups" then
                if syslinks:has(value) then
                    kind = "syslinks"
                elseif frameworks:has(value) then
                    kind = "frameworks"
                end
            end

            -- if we link to a target, take it's kind
            if is_target(value) then
                kind = project.target(filename_map[value]):kind()
            end

            if not kind then
                kind = _find_kind(value, linkdirs)
            end
            table.insert(links, {name = value, kind = kind})
        end
        ::continue::
    end

    return links
end

function _link_info(targets, opt)
    opt = opt or {}

    local xmake_rs = localcache.cache("xmake_rs")
    local cache_key = utils.get_cache_key(targets)

    local in_cache = xmake_rs:get2("linkinfo", cache_key) ~= nil
    if (not opt.recheck) and in_cache then
        local linkinfo = xmake_rs:get2("linkinfo", cache_key)
        local linkdirs = linkinfo.linkdirs
        local links = linkinfo.links

        return {links = links, linkdirs = linkdirs}
    end

    local binary_target = utils.create_binary_target(targets)
    local links = _get_links(binary_target)
    local linkdirs = _get_linkdirs(binary_target, {runenvs = true})
    
    xmake_rs:set2("linkinfo", cache_key, {linkdirs = linkdirs, links = links})
    xmake_rs:save()

    return {links = links, linkdirs = linkdirs}
end

function _stl_usage(target, sourcebatch, opt)
    opt = opt or {}
    local xmake_rs = localcache.cache("xmake_rs")
    local modules_cache = localcache.cache("cxxmodules")

    -- collect the files that use the stl previously
    local files = xmake_rs:get2("stl", target:name()) or {}
    files = hashset.from(files)

    -- wrap the on_changed callback
    local stl_detection = function(index, sourcefile, callback)
        local dependfile = target:dependfile(target:objectfile(sourcefile))
        local result = false

        depend.on_changed(function()
            result = callback(index)
        end, {dependfile = dependfile, files = {sourcefile}, changed = target:is_rebuilt()})
        return result
    end

    local process_modules_files = function(index)
        local sourcefile = sourcebatch.sourcefiles[index]
        local objectfile = sourcebatch.objectfiles[index]

        local fileinfo = modules_cache:get3("modules",  target:name() .. "/" .. "c++.build.modules.builder", objectfile)
        assert(fileinfo, "cxxmodules cache is empty. build the the target first")

        local requires = fileinfo.requires
        for require, v in pairs(requires) do
            -- import std;
            if require == "std" or require == "std.compat" then
                files:insert(sourcefile)
                return true -- signal to stop 
            end

            -- import <iostream>;
            if utils.is_stl_used(target, require) then
                files:insert(sourcefile)
                return true -- signal to stop
            end
        end

        -- the file doesn't use the stl anymore
        if files:has(sourcefile) then
            files:remove(sourcefile)
        end
        return false
    end

    local process_files = function(index)
        local sourcefile = sourcebatch.sourcefiles[index]
        local includes = _include_scanner(target).scan(target, sourcefile, opt)

        if utils.is_stl_used(target, includes, {strict = true}) then
            files:insert(sourcefile)
            return true -- signal to stop
        end

        -- the file doesn't use the stl anymore
        if files:has(sourcefile) then
            files:remove(sourcefile)
        end
        return false
    end

    local process_files = opt.modules and process_modules_files or process_files

    if opt.batchjobs then
        -- if a project use the stl it will detected in the first few files
        -- so a large jobs number is not necessary
        local jobs = 2 
        try {
            function ()
                runjobs(target:name() .. "_stl_scanner", function(index)
                    if stl_detection(index, sourcebatch.sourcefiles[index], process_files) then
                        raise() -- little hack to stop the async jobs
                    end
                end, {comax = jobs, total = #sourcebatch.sourcefiles})
            end
        }
    else
        for index, sourcefile in ipairs(sourcebatch.sourcefiles) do
            if stl_detection(index, sourcebatch.sourcefiles[index], process_files) then
                break
            end
        end
      
    end

    xmake_rs:set2("stl", target:name(), files:to_array())
    xmake_rs:save()
    return files:size() > 0
end

function _stl_info(targets)
    local is_cxx_used = false
    local is_stl_used = false

    for _, target in pairs(targets) do
        local sourcebatches, _ = target:sourcebatches()
        local is_cxx = sourcebatches["c++.build"] ~= nil
        local is_cxx_modules = sourcebatches["c++.build.modules.builder"] ~= nil

        is_cxx_used = is_cxx or is_cxx_modules
        if is_cxx then
            is_stl_used = _stl_usage(target, sourcebatches["c++.build"], {batchjobs = true})
        end

        if is_cxx_modules then
            is_stl_used = is_stl_used or _stl_usage(target, sourcebatches["c++.build.modules.builder"], {modules = true, batchjobs = true})
        end

        if is_stl_used then
            break
        end
    end

    return {cxx_used = is_cxx_used, stl_used = is_stl_used}
end


function main()
    -- load the config to get the correct options
    local oldir = os.cd(os.projectdir())
    config.load()
    project.load_targets()
    -- task.run("config")

    local recheck = _cache_invalidation()
    local targets, _ = utils.get_targets()

    local infos = _link_info(targets, {recheck = recheck})
    table.join2(infos, _stl_info(targets, {recheck = recheck}))

    -- print(infos)
    _print_infos(infos)
    os.cd(oldir)
end