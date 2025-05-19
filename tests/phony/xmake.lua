
add_repositories("xmakers-repo https://github.com/A2va/xmakers-repo")
add_requires("xmrs-foo")

target("bar")
    set_kind("phony")
    add_packages("xmrs-foo", {public = true})
