local project_root = path.absolute("../../..", os.scriptdir())
local repo_path = path.join(project_root, "xmakers-repo")

add_repositories(format("xmakers-repo %s", repo_path))
add_requires("xmrs.foo")

target("bar")
    set_kind("$(kind)")
    add_files("bar.c")
    add_headerfiles("bar/bar.h", {prefixdir = "bar"})

    add_packages("xmrs.foo")

    add_defines("BAR_BUILD")
    if is_kind("static") then
        add_defines("BAR_STATIC", {public = true})
    end