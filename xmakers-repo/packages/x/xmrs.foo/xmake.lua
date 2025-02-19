package("xmrs.foo")
    set_description("Test package for the xmake-rs test suite")
    set_sourcedir(path.join(path.absolute("../../..", os.scriptdir()), "projects", "foo"))

    on_install(function (package)
        if not package:config("shared") then
            package:add("defines", "FOO_STATIC")
        end  
        import("package.tools.xmake").install(package)
    end)

    on_test(function (package)
        assert(package:check_csnippets({test = [[
            void test() {
               int f = foo();
            }
        ]]}, {includes = "foo/foo.h"}))
    end)