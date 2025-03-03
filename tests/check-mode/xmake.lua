target("foo")
    set_kind("static")
    add_files("src/foo.c")
    if is_mode("debug") then
        add_defines("FOO_DEBUG")
    elseif is_mode("release") then
        add_defines("FOO_RELEASE")
    end