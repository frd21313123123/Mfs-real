$ErrorActionPreference = 'Stop'
$taskRoot = Split-Path -Parent $PSScriptRoot
$taskNode = (Get-Command node -ErrorAction Stop).Source
$taskPython = Join-Path $env:USERPROFILE '.cache\codex-runtimes\codex-primary-runtime\dependencies\python\python.exe'
if (-not (Test-Path -LiteralPath $taskPython)) { $taskPython = (Get-Command python -ErrorAction Stop).Source }
$taskConfig = Join-Path $taskRoot '.local'
New-Item -ItemType Directory -Path $taskConfig -Force | Out-Null
@{ node = $taskNode; python = $taskPython } | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $taskConfig 'runtime.json') -Encoding utf8
& $taskPython (Join-Path $PSScriptRoot 'pdf_tools.py') check
if ($LASTEXITCODE -ne 0) { throw 'Install reportlab and pypdfium2 in the selected Python environment.' }
