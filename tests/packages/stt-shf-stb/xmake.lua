set_policy("package.librarydeps.strict_compatibility", true)
add_repositories("xmakers-repo https://github.com/A2va/xmakers-repo")

add_requires("xmrs-bar", {configs = {shared = false}})
add_requireconfs("xmrs-bar.xmrs-foo", {configs = {shared = true}})

target("target")
    set_kind("static")
    add_files("src/target.c")
    add_packages("xmrs-bar")