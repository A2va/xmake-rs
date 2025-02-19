-- Copyright (c) 2024 A2va

-- Permission is hereby granted, free of charge, to any
-- person obtaining a copy of this software and associated
-- documentation files (the "Software"), to deal in the
-- Software without restriction, including without
-- limitation the rights to use, copy, modify, merge,
-- publish, distribute, sublicense, and/or sell copies of
-- the Software, and to permit persons to whom the Software
-- is furnished to do so, subject to the following
-- conditions:

-- The above copyright notice and this permission notice
-- shall be included in all copies or substantial portions
-- of the Software.

-- THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF
-- ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED
-- TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A
-- PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT
-- SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
-- CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
-- OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR
-- IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
-- DEALINGS IN THE SOFTWARE.

function _extract_includes(content)
    local includes = {}
    for line in string.gmatch(content, "[^\r\n]+") do
        -- extract file paths from the lines that contain include files
        for include in string.gmatch(line, "/[%a%d%p\\]+") do
            table.insert(includes, path.normalize(include))
        end
    end
    -- remove the source file itself
    table.remove(includes, 1)
    return includes
end

function scan(target, sourcefile, opt)
    local compinst = target:compiler("cxx")
    local ifile = path.translate(path.join(outputdir, sourcefile .. ".i"))

    local compflags = compinst:compflags({sourcefile = sourcefile, target = target})
    local flags = table.join(compflags, {"-M", "-c", "-x", "c++", sourcefile})

    local content,  _ = os.iorunv(compinst:program(), flags)
    return _extract_includes(content)
end