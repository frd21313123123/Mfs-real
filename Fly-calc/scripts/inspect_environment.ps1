$ErrorActionPreference = 'Stop'
$taskRoot = Split-Path -Parent $PSScriptRoot
$taskTools = @{}
foreach ($taskName in @('node','python','git','docker','wsl')) {
  $taskCommand = Get-Command $taskName -ErrorAction SilentlyContinue
  $taskTools[$taskName] = if ($taskCommand) { $taskCommand.Source } else { $null }
}
$taskKnownConfigs = @(
  "$env:APPDATA\Microsoft Flight Simulator\UserCfg.opt",
  "$env:APPDATA\Microsoft Flight Simulator 2024\UserCfg.opt",
  "$env:LOCALAPPDATA\Packages\Microsoft.FlightSimulator_8wekyb3d8bbwe\LocalCache\UserCfg.opt",
  "$env:LOCALAPPDATA\Packages\Microsoft.Limitless_8wekyb3d8bbwe\LocalCache\UserCfg.opt"
)
$taskFoundConfigs = @($taskKnownConfigs | Where-Object { Test-Path -LiteralPath $_ })
$taskFenixPaths = @('C:\Program Files\FenixSim A320','C:\ProgramData\Fenix\FenixSim A320')
$taskResult = @{
  checkedAt = '2026-10-09'; tools = $taskTools;
  simulatorConfigsFound = $taskFoundConfigs;
  fenixInstallationsFound = @($taskFenixPaths | Where-Object { Test-Path -LiteralPath $_ });
  simulatorSearchStatus = 'No simulator found in checked default locations; not an exhaustive disk scan';
  fullFbwBuild = 'not_run'; inSimulator2020 = 'not_run'; inSimulator2024 = 'not_run'; fenixCockpit = 'not_run'
}
$taskResult | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $taskRoot 'evidence\environment.json') -Encoding utf8
Write-Output 'Environment check recorded; simulator runtime checks remain unperformed.'
