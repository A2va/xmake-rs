set_policy("compatibility.version", "3.0")

namespace("ns1", function ()
    target("foo")
        set_kind("static")
        add_files("src/foo.c")
end)