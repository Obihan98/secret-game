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

# A step with `MustReject` passes only if the command FAILS and its output names every ban;
# that proves the determinism lints in clippy.toml are live, not just configured.
$steps = @(
	@{ Name = 'fmt'; Exe = 'cargo'; Args = @('fmt', '--check') },
	@{ Name = 'clippy'; Exe = 'cargo'; Args = @(
			'clippy', '--workspace', '--all-targets', '--', '-D', 'warnings') },
	@{ Name = 'lint-bans'; Exe = 'cargo'; Args = @(
			'clippy', '--manifest-path', 'scripts/lint_bans/Cargo.toml',
			'--target-dir', 'target/lint_bans', '--', '-D', 'warnings')
		MustReject = @(
			'disallowed type `std::collections::HashMap`',
			'disallowed type `std::collections::HashSet`',
			'disallowed method `std::time::Instant::now`',
			'disallowed method `std::time::SystemTime::now`') },
	@{ Name = 'test'; Exe = 'cargo'; Args = @('test', '--workspace') },
	@{ Name = 'godot'; Exe = 'godot'; Args = @('--headless', '--path', 'client', '--quit') }
)

foreach ($step in $steps) {
	Write-Host "==> $($step.Name): $($step.Exe) $($step.Args -join ' ')"
	if ($step.MustReject) {
		# Native stderr through 2>&1 becomes error records in PowerShell 5.1; don't throw on them.
		$ErrorActionPreference = 'Continue'
		$out = & $step.Exe @($step.Args) 2>&1 | Out-String
		$code = $LASTEXITCODE
		$ErrorActionPreference = 'Stop'
		$missing = @($step.MustReject | Where-Object { -not $out.Contains($_) })
		if ($code -eq 0 -or $missing.Count -gt 0) {
			Write-Host $out
			Write-Host "FAIL $($step.Name) (exit $code; not rejected: $($missing -join ', '))" `
				-ForegroundColor Red
			exit 1
		}
	} else {
		& $step.Exe @($step.Args)
		if ($LASTEXITCODE -ne 0) {
			Write-Host "FAIL $($step.Name) (exit $LASTEXITCODE)" -ForegroundColor Red
			exit 1
		}
	}
	Write-Host "PASS $($step.Name)" -ForegroundColor Green
}

Write-Host 'ALL PASS' -ForegroundColor Green
exit 0
