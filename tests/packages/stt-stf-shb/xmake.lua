add_repositories("xmakers-repo https://github.com/A2va/xmakers-repo")

add_requires("xmrs-bar", {configs = {shared = true}})
add_requireconfs("xmrs-bar.xmrs-foo", {configs = {shared = false}})

target("target")
    set_kind("static")
    add_files("src/target.c")
    add_packages("xmrs-bar")