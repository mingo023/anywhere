# Installs pocketd @@VERSION@@ on a Windows host and starts it as a logon task. PowerShell 5.1.
# The desktop reads the "ANYWHERE step|error|done" lines.
$ErrorActionPreference = 'Stop'
# The progress bar makes Invoke-WebRequest many times slower on 5.1.
$ProgressPreference = 'SilentlyContinue'
$Version = '@@VERSION@@'
$Url = '@@URL@@'
$HomeDir = Join-Path $HOME '@@HOME_DIR@@'
$Label = '@@LABEL@@'
$Port = if ('@@HOME_DIR@@'.EndsWith('-dev')) { 4520 } else { 4519 }

function Step($name) { Write-Output "ANYWHERE step $name" }

function Answers {
  $c = New-Object Net.Sockets.TcpClient
  try { return $c.ConnectAsync('127.0.0.1', $Port).Wait(500) } catch { return $false } finally { $c.Close() }
}

try {
  Step 'detect'
  # A 32-bit shell on ARM64 reports x86 and the real arch here.
  $cpu = if ($env:PROCESSOR_ARCHITEW6432) { $env:PROCESSOR_ARCHITEW6432 } else { $env:PROCESSOR_ARCHITECTURE }
  $arch = switch ($cpu) { 'AMD64' { 'amd64' } 'ARM64' { 'arm64' } default { throw "unsupported CPU $cpu" } }
  if (Answers) {
    Write-Output 'ANYWHERE done'
    exit 0
  }

  $zip = "pocketd-windows-$arch.zip"
  $tmp = Join-Path ([IO.Path]::GetTempPath()) ("anywhere-" + [Guid]::NewGuid())
  New-Item -ItemType Directory -Path $tmp | Out-Null
  try {
    if ($env:ANYWHERE_ARCHIVE) {
      Step 'copy'
      $archive = $env:ANYWHERE_ARCHIVE
      $sum = "$archive.sha256"
    } else {
      Step 'download'
      [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12
      $archive = Join-Path $tmp $zip
      $sum = "$archive.sha256"
      Invoke-WebRequest -UseBasicParsing -Uri "$Url/$zip" -OutFile $archive
      Invoke-WebRequest -UseBasicParsing -Uri "$Url/$zip.sha256" -OutFile $sum
    }

    Step 'verify'
    $want = ((Get-Content -Raw $sum).Trim() -split '\s+')[0]
    $got = (Get-FileHash -Algorithm SHA256 $archive).Hash
    if ($got -ne $want) { throw "$zip checksum is $got, want $want" }

    Step 'unpack'
    $runtime = Join-Path $HomeDir "runtime\$Version"
    New-Item -ItemType Directory -Force -Path $runtime | Out-Null
    Expand-Archive -Force -Path $archive -DestinationPath $runtime
  } finally {
    Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue
  }

  Step 'install'
  & (Join-Path $runtime 'pocketd.exe') service install --label $Label
  if ($LASTEXITCODE -ne 0) { throw "pocketd service install exited $LASTEXITCODE" }

  Step 'start'
  $deadline = (Get-Date).AddSeconds(30)
  while (-not (Answers)) {
    if ((Get-Date) -gt $deadline) { throw "pocketd didn't listen on 127.0.0.1:$Port within 30s" }
    Start-Sleep -Milliseconds 500
  }
  Write-Output 'ANYWHERE done'
} catch {
  Write-Output "ANYWHERE error $($_.Exception.Message)"
  exit 1
}
