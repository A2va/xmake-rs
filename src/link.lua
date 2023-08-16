
import("core.project.project")
import("core.base.task")

function _get_values_from_target(target, name)
    local values = table.wrap(target:get(name))
    table.join2(values, target:get_from_opts(name))
    table.join2(values, target:get_from_pkgs(name))
    table.join2(values, target:get_from_deps(name, {interface = true}))
    return table.unique(values)
end

function _print_output(targets, name, output)
    local values = {}
    for _, target in pairs(targets) do
        if target:is_library() then
            table.join2(values, _get_values_from_target(target, name))
        end
    end

    print(format("%s:", output) .. table.concat(table.unique(values), "|"))
end

function main(argv)
    -- Enter project directory
    local oldir = os.cd(os.projectdir())
    -- Run the configure task to get the lastest links
    task.run("config")

    local targets = project.targets()
    if os.getenv("TARGET") then
        targets = {project.target(os.getenv("TARGET"))}
    end

    _print_output(targets, "linkdirs", "linkd")
    _print_output(targets, "links", "links")
    _print_output(targets, "syslinks", "syslk")

    -- Leave project directory
    os.cd(oldir)
end
