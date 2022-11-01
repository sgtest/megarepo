#!/usr/bin/env pwsh

# See ./x for why these scripts exist.

$xpy = Join-Path $PSScriptRoot x.py
# Start-Process for some reason splits arguments on spaces. (Isn't powershell supposed to be simpler than bash?)
# Double-quote all the arguments so it doesn't do that.
$xpy_args = @("""$xpy""")
foreach ($arg in $args) {
    $xpy_args += """$arg"""
}

function Get-Application($app) {
    return Get-Command $app -ErrorAction SilentlyContinue -CommandType Application
}

foreach ($python in "py", "python3", "python", "python2") {
    # NOTE: this only tests that the command exists in PATH, not that it's actually
    # executable. The latter is not possible in a portable way, see
    # https://github.com/PowerShell/PowerShell/issues/12625.
    if (Get-Application $python) {
        if ($python -eq "py") {
            # Use python3, not python2
            $xpy_args = @("-3") + $xpy_args
        }
        $process = Start-Process -NoNewWindow -Wait -PassThru $python $xpy_args
        Exit $process.ExitCode
    }
}

$found = (Get-Application "python*" | Where-Object {$_.name -match '^python[2-3]\.[0-9]+(\.exe)?$'})
if (($null -ne $found) -and ($found.Length -ge 1)) {
    $python = $found[0]
    $process = Start-Process -NoNewWindow -Wait -PassThru $python $xpy_args
    Exit $process.ExitCode
}

Write-Error "${PSCommandPath}: error: did not find python installed"
Exit 1
