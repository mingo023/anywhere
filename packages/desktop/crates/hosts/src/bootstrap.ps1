# Installs pocketd @@VERSION@@ on a Windows host and starts it as a logon task. PowerShell 5.1.
# The desktop fills the @@...@@ placeholders and pipes this to PowerShell over ssh. Keep it ASCII:
# PowerShell reads stdin in the console's code page.
# ANYWHERE_ARCHIVE names a zip already on the host to use instead (checked against ANYWHERE_SHA256 if set), removed after.
# ANYWHERE_FORCE=1 updates: replaces a running pocketd, which closes its terminals.
# Lines for the desktop: ANYWHERE step <name> | error <msg> | running | done. Others are log.
$ErrorActionPreference = 'Stop'
# The progress bar makes Invoke-WebRequest many times slower on 5.1.
$ProgressPreference = 'SilentlyContinue'
$Version = '@@VERSION@@'
$Url = '@@URL@@'
$HomeDir = Join-Path $HOME '@@HOME_DIR@@'
$Label = '@@LABEL@@'
# pocketd reads its home from here when the shim, service install and pair run it.
$env:POCKET_HOME = $HomeDir

function Step($name) { Write-Output "ANYWHERE step $name" }

# The owner port pocketd listens on: config.json's ownerListen, else a release build's default.
function Port {
  $config = Join-Path $HomeDir 'config.json'
  if (Test-Path $config) {
    $listen = (Get-Content -Raw $config | ConvertFrom-Json).ownerListen
    if ($listen -and $listen -ne 'off') { return [int]($listen -split ':')[-1] }
  }
  return 4519
}

function Answers {
  $c = New-Object Net.Sockets.TcpClient
  try { return $c.ConnectAsync('127.0.0.1', (Port)).Wait(500) } catch { return $false } finally { $c.Close() }
}

try {
  Step 'detect'
  # A 32-bit shell on ARM64 reports x86 and the real arch here.
  $cpu = if ($env:PROCESSOR_ARCHITEW6432) { $env:PROCESSOR_ARCHITEW6432 } else { $env:PROCESSOR_ARCHITECTURE }
  $arch = switch ($cpu) { 'AMD64' { 'amd64' } 'ARM64' { 'arm64' } default { throw "unsupported CPU $cpu" } }
  if (Answers) {
    if (-not $env:ANYWHERE_FORCE) {
      Write-Output 'ANYWHERE running'
      exit 0
    }
    # The task runs one instance, and the running one holds its pocketd.exe open.
    $ErrorActionPreference = 'Continue'
    schtasks.exe /End /TN $Label 2>&1 | Out-Null
    $ErrorActionPreference = 'Stop'
    $deadline = (Get-Date).AddSeconds(15)
    while (Answers) {
      if ((Get-Date) -gt $deadline) { throw "pocketd didn't stop within 15s" }
      Start-Sleep -Milliseconds 300
    }
  }

  $zip = "pocketd-windows-$arch.zip"
  $tmp = Join-Path ([IO.Path]::GetTempPath()) ("anywhere-" + [Guid]::NewGuid())
  New-Item -ItemType Directory -Path $tmp | Out-Null
  try {
    $want = $env:ANYWHERE_SHA256
    if ($env:ANYWHERE_ARCHIVE) {
      $archive = $env:ANYWHERE_ARCHIVE
    } else {
      Step 'download'
      [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12
      $archive = Join-Path $tmp $zip
      Invoke-WebRequest -UseBasicParsing -Uri "$Url/$zip" -OutFile $archive
      Invoke-WebRequest -UseBasicParsing -Uri "$Url/$zip.sha256" -OutFile "$archive.sha256"
      $want = ((Get-Content -Raw "$archive.sha256").Trim() -split '\s+')[0]
    }

    if ($want) {
      Step 'verify'
      $got = (Get-FileHash -Algorithm SHA256 $archive).Hash
      if ($got -ne $want) { throw "$zip checksum is $got, want $want" }
    }

    Step 'unpack'
    $runtime = Join-Path $HomeDir "runtime\$Version"
    New-Item -ItemType Directory -Force -Path $runtime | Out-Null
    Expand-Archive -Force -Path $archive -DestinationPath $runtime
  } finally {
    Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue
    if ($env:ANYWHERE_ARCHIVE) { Remove-Item -Force $env:ANYWHERE_ARCHIVE -ErrorAction SilentlyContinue }
  }

  # A stable path for pair and uninstall: pocketd.exe's folder changes with each version.
  Step 'link'
  $exe = Join-Path $runtime 'pocketd.exe'
  $bin = Join-Path $HomeDir 'bin'
  New-Item -ItemType Directory -Force -Path $bin | Out-Null
  [IO.File]::WriteAllText((Join-Path $bin 'pocketd.cmd'), "@set `"POCKET_HOME=$HomeDir`"`r`n@`"$exe`" %*`r`n")

  Step 'service'
  & $exe service install --label $Label
  if ($LASTEXITCODE -ne 0) { throw "pocketd service install exited $LASTEXITCODE" }

  Step 'start'
  $deadline = (Get-Date).AddSeconds(30)
  while (-not (Answers)) {
    if ((Get-Date) -gt $deadline) { throw "pocketd didn't listen on 127.0.0.1:$(Port) within 30s" }
    Start-Sleep -Milliseconds 500
  }
  Write-Output 'ANYWHERE done'
} catch {
  Write-Output "ANYWHERE error $($_.Exception.Message)"
  exit 1
}
