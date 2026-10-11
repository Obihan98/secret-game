# The stage gate (docs/tech/build-order.md): every step must pass before a commit.
# Stops at the first failing step and exits non-zero.

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

# New terminals may not see rustup's bin directory yet.
$cargoBin = Join-Path $HOME '.cargo\bin'
if (($env:PATH -split ';') -notcontains $cargoBin) {
	$env:PATH = "$cargoBin;$env:PATH"
}

$steps = @(
	@{ Name = 'fmt'; Exe = 'cargo'; Args = @('fmt', '--check') },
	@{ Name = 'clippy'; Exe = 'cargo'; Args = @(
			'clippy', '--workspace', '--all-targets', '--', '-D', 'warnings') },
	@{ Name = 'test'; Exe = 'cargo'; Args = @('test', '--workspace') },
	@{ Name = 'godot'; Exe = 'godot'; Args = @('--headless', '--path', 'client', '--quit') }
)

foreach ($step in $steps) {
	Write-Host "==> $($step.Name): $($step.Exe) $($step.Args -join ' ')"
	& $step.Exe @($step.Args)
	if ($LASTEXITCODE -ne 0) {
		Write-Host "FAIL $($step.Name) (exit $LASTEXITCODE)" -ForegroundColor Red
		exit 1
	}
	Write-Host "PASS $($step.Name)" -ForegroundColor Green
}

Write-Host 'ALL PASS' -ForegroundColor Green
exit 0
