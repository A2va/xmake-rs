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
import("core.project.config")
import("core.project.project")

import("core.base.hashset")
import("core.base.task")
import("target.action.install")

import("modules.utils")

function main()
    local oldir = os.cd(os.projectdir())

    config.load()
    project.load_targets()

    local targets, _ = utils.get_targets()
    local binary_target = utils.create_binary_target(targets)

    -- create a dummy executable file
    -- because this excutable is not existent xmake cannot check which dlls are dependants so disable stripping
    if not os.exists(path.directory(binary_target:targetfile())) then
        os.mkdir(path.directory(binary_target:targetfile()))
    end
    os.touch(binary_target:targetfile())
    memcache.cache("core.project.project"):set2("policies", "install.strip_packagelibs", false)
    binary_target:set("installdir", os.getenv("XMAKERS_INSTALL_DIR"))
    install(binary_target)

    os.cd(oldir)
end