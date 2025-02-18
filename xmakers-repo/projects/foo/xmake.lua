target("foo")
    set_kind("$(kind)")
    add_files("foo.c")
    add_headerfiles("foo/foo.h", {prefixdir = "foo"})

    add_defines("FOO_BUILD")
    if is_kind("static") then
        add_defines("FOO_STATIC", {public = true})
    end