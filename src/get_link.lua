import("core.project.project")
import("core.base.task")

function _get_values_from_target(target, name)
    local values = table.wrap(target:get(name))
    table.join2(values, target:get_from_opts(name))
    table.join2(values, target:get_from_pkgs(name))
    table.join2(values, target:get_from_deps(name, {interface = true}))
    return table.unique(values)
end

function _print_output(targets, name)
    local values = {}
    for _, target in pairs(targets) do
        if target:is_library() then
            table.join2(values, _get_values_from_target(target, name))
        end
    end

    if name == "linkdirs" then
        for i, v in ipairs(values) do 
            -- If a path is relative it's certainly relative to project directory
            if not path.is_absolute(v) then
                values[i] = path.join(project.directory(), v)
            end
        end
    end

    print(format("%s:", name) .. table.concat(table.unique(values), "|"))
end

function main(argv)
    -- Enter project directory
    local oldir = os.cd(os.projectdir())
    -- Run the configure task
    task.run("config")

    local targets = project.targets()
    if os.getenv("TARGET") then
        targets = {project.target(os.getenv("TARGET"))}
    end

    _print_output(targets, "linkdirs")
    _print_output(targets, "links")
    _print_output(targets, "syslinks")

    _print_output(targets, "frameworks")


    -- Leave project directory
    os.cd(oldir)
end