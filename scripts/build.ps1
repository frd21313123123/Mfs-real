# Run from any directory. Requires Rust 1.90 and Visual Studio C++ Build Tools only to BUILD.
$ErrorActionPreference = 'Stop'
Push-Location (Join-Path $PSScriptRoot '..')
try {
    function Invoke-Cargo {
        & cargo @args
        if ($LASTEXITCODE -ne 0) { throw "cargo failed with exit code $LASTEXITCODE" }
    }
    Invoke-Cargo fmt --all -- --check
    Invoke-Cargo clippy --locked --all-targets --target x86_64-pc-windows-msvc -- -D warnings
    Invoke-Cargo test --locked --all-targets --target x86_64-pc-windows-msvc
    Invoke-Cargo build --release --locked --bins --target x86_64-pc-windows-msvc
    $buildStage = Join-Path (Get-Location) ('release/.realflow-stage-' + [guid]::NewGuid().ToString('N'))
    $package = Join-Path $buildStage 'RealFlow-Windows-x64'
    New-Item -ItemType Directory -Force -Path $package | Out-Null
    Copy-Item target/x86_64-pc-windows-msvc/release/realflow.exe $package -Force
    Copy-Item target/x86_64-pc-windows-msvc/release/realflow-launcher.exe $package -Force
    Copy-Item README.md,LICENSE,config.json,start-launcher.bat,run-demo.bat,start-msfs-experimental.bat $package -Force
    New-Item -ItemType Directory -Force -Path (Join-Path $package 'examples') | Out-Null
    Copy-Item examples/* (Join-Path $package 'examples') -Recurse -Force
    # Smoke the distribution from an unrelated working directory; Python is never invoked.
    Push-Location $env:TEMP
    try {
        & "$package/realflow.exe" --config "$package/config.json" doctor
        if ($LASTEXITCODE -ne 0) { throw 'Packaged doctor failed' }
        & "$package/realflow.exe" --config "$package/config.json" demo --steps 240 --output "$package/demo-results.json"
        if ($LASTEXITCODE -ne 0) { throw 'Packaged demo failed' }
        & "$package/realflow.exe" --config "$package/config.json" atc-demo --steps 600 --output "$package/atc-demo-results.json"
        if ($LASTEXITCODE -ne 0) { throw 'Packaged ATC demo failed' }
    } finally { Pop-Location }
    Compress-Archive -Path "$package/*" -DestinationPath release/RealFlow-Windows-x64.zip -Force
    $hash = (Get-FileHash release/RealFlow-Windows-x64.zip -Algorithm SHA256).Hash.ToLowerInvariant()
    "$hash  RealFlow-Windows-x64.zip" | Set-Content release/SHA256SUMS.txt -Encoding ascii
    Write-Host 'Ready: release/RealFlow-Windows-x64.zip (no Python required)'
 } finally {
    if ($buildStage -and (Test-Path $buildStage)) { Remove-Item -LiteralPath $buildStage -Recurse -Force }
    Pop-Location
}
