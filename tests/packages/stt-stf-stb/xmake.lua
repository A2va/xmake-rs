-- local project_root = path.absolute("../../..", os.scriptdir())
-- local repo_path = path.join(project_root, "xmakers-repo")

-- add_repositories(format("xmakers-repo %s", repo_path))
add_repositories("xmakers-repo https://github.com/A2va/xmakers-repo")

add_requires("xmrs-bar", {configs = {shared = false}})
add_requireconfs("xmrs-bar.xmrs-foo", {configs = {shared = false}})

target("target")
    set_kind("static")
    add_files("src/target.c")
    add_packages("xmrs-bar")