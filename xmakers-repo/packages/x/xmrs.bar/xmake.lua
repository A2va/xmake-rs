package("xmrs.bar")
    set_description("Test package for the xmake-rs test suite")
    set_sourcedir(path.join(path.absolute("../../..", os.scriptdir()), "projects", "bar"))

    add_deps("xmrs.foo")

    on_install(function (package)
        if not package:config("shared") then
            package:add("defines", "BAR_STATIC")
        end    
        import("package.tools.xmake").install(package)
    end)

    on_test(function (package)
        assert(package:check_csnippets({test = [[
            void test() {
               int b = bar();
            }
        ]]}, {includes = "bar/bar.h"}))
    end)    