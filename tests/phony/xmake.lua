
add_repositories("xmakers-repo https://github.com/A2va/xmakers-repo")
add_requires("xmrs-foo")

target("phony")
    set_kind("phony")
    add_packages("xmrs-foo", {public = true})
