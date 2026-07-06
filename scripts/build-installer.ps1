# Video Summary Bot — Windows 安装包构建脚本
# 产出：dist/VideoSummaryBot-Setup-0.1.0.exe
#
# 依赖：
#   - Rust toolchain
#   - Inno Setup 6（自动尝试 winget 安装）

$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
Set-Location $Root

$Version = (Select-String -Path "Cargo.toml" -Pattern '^version = "(.+)"' | Select-Object -First 1).Matches.Groups[1].Value
$Staging = Join-Path $Root "dist\staging"
$OutDir = Join-Path $Root "dist"

Write-Host "=== Video Summary Bot Installer Build v$Version ===" -ForegroundColor Cyan

# 1. Release 编译（含 GUI）
Write-Host "`n[1/4] cargo build --release --features gui ..." -ForegroundColor Yellow
cargo build --release --features gui
if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }

# 2. 准备 staging 目录
Write-Host "`n[2/4] staging files ..." -ForegroundColor Yellow
if (Test-Path $Staging) { Remove-Item $Staging -Recurse -Force }
New-Item -ItemType Directory -Path (Join-Path $Staging "gui\web") -Force | Out-Null

Copy-Item "target\release\video-summary-bot.exe" $Staging
Copy-Item "gui\web\*" (Join-Path $Staging "gui\web") -Recurse

@"
Video Summary Bot v$Version
========================

启动方式：
  - 开始菜单 → Video Summary Bot
  - 或命令行：video-summary-bot gui

系统要求：
  - Windows 10/11 x64
  - Microsoft WebView2 Runtime（Win11 通常已预装）
    下载：https://developer.microsoft.com/microsoft-edge/webview2/

VM 镜像（可选）：
  在项目源码目录运行 scripts/prepare-guest.sh 生成 assets/

"@ | Set-Content -Path (Join-Path $Staging "README.txt") -Encoding UTF8

# 3. 查找 Inno Setup 编译器
Write-Host "`n[3/4] locating Inno Setup (ISCC.exe) ..." -ForegroundColor Yellow
$IsccCandidates = @(
    "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe",
    "$env:ProgramFiles\Inno Setup 6\ISCC.exe",
    "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe"
)
$Iscc = $IsccCandidates | Where-Object { Test-Path $_ } | Select-Object -First 1

if (-not $Iscc) {
    Write-Host "Inno Setup not found, trying winget install ..." -ForegroundColor Yellow
    winget install --id JRSoftware.InnoSetup -e --accept-package-agreements --accept-source-agreements 2>$null
    $Iscc = $IsccCandidates | Where-Object { Test-Path $_ } | Select-Object -First 1
}

if (-not $Iscc) {
    Write-Host "`nERROR: Inno Setup 6 not installed." -ForegroundColor Red
    Write-Host "Please install from: https://jrsoftware.org/isdl.php" -ForegroundColor Red
    Write-Host "Then re-run: .\scripts\build-installer.ps1" -ForegroundColor Red
    Write-Host "`nStaging folder ready at: $Staging" -ForegroundColor Yellow
    exit 1
}

Write-Host "Using: $Iscc" -ForegroundColor Green

# 4. 编译安装程序
Write-Host "`n[4/4] compiling installer ..." -ForegroundColor Yellow
New-Item -ItemType Directory -Path $OutDir -Force | Out-Null
& $Iscc "installer\vsb-setup.iss"
if ($LASTEXITCODE -ne 0) { throw "ISCC failed" }

$SetupExe = Join-Path $OutDir "VideoSummaryBot-Setup-$Version.exe"
if (Test-Path $SetupExe) {
    $Size = [math]::Round((Get-Item $SetupExe).Length / 1MB, 2)
    Write-Host "`nSUCCESS: $SetupExe ($Size MB)" -ForegroundColor Green
} else {
    throw "Installer not found at expected path"
}