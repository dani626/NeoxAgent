$ErrorActionPreference = "Stop"

# Configuration
$BaseUrl = "http://127.0.0.1:8443"
$ApiKey = "change-me-to-a-secure-random-string"
$Headers = @{
    "Authorization" = "Bearer $ApiKey"
    "Content-Type"  = "application/json"
}

# Helper Function
function Test-Endpoint {
    param (
        [string]$Method,
        [string]$Path,
        [string]$Title,
        [Hashtable]$Body = $null
    )

    Write-Host "Testing: $Title ($Method $Path)..." -NoNewline
    
    try {
        if ($Body) {
            $JsonBody = $Body | ConvertTo-Json -Depth 10 -Compress
            $Response = Invoke-RestMethod -Uri "$BaseUrl$Path" -Method $Method -Headers $Headers -Body $JsonBody -ErrorAction Stop
        } else {
            $Response = Invoke-RestMethod -Uri "$BaseUrl$Path" -Method $Method -Headers $Headers -ErrorAction Stop
        }
        Write-Host " [OK] ✅" -ForegroundColor Green
        return $Response
    } catch {
        Write-Host " [FAILED] ❌" -ForegroundColor Red
        Write-Host $_.Exception.Message -ForegroundColor Yellow
        if ($_.Exception.Response) {
            $Stream = $_.Exception.Response.GetResponseStream()
            $Reader = New-Object System.IO.StreamReader($Stream)
            $Reader.ReadToEnd() | Write-Host -ForegroundColor Yellow
        }
        return $null
    }
}

Write-Host "🚀 Starting NeoxAgent Integration Tests" -ForegroundColor Cyan
Write-Host "=========================================" -ForegroundColor Cyan

# 1. Health Check
$Health = Test-Endpoint -Method GET -Path "/api/health" -Title "Health Check"

# 2. Phase 7: Pull Image (Preparation)
Test-Endpoint -Method POST -Path "/api/images/pull" -Title "Pull Alpine Image" -Body @{ image = "docker.io/library/alpine:latest" } | Out-Null

# 3. Phase 1: Create Container
$ContainerReq = @{
    name = "test-container-01"
    image = "docker.io/library/alpine:latest"
    command = @("sleep", "3600")
}
$Container = Test-Endpoint -Method POST -Path "/api/containers" -Title "Create Container" -Body $ContainerReq

if ($Container) {
    $ContainerId = $Container.id
    Test-Endpoint -Method GET -Path "/api/containers/$ContainerId" -Title "Inspect Container" | Out-Null
    Test-Endpoint -Method POST -Path "/api/containers/$ContainerId/stop" -Title "Stop Container" | Out-Null
    Test-Endpoint -Method DELETE -Path "/api/containers/$ContainerId?force=true" -Title "Delete Container" | Out-Null
}

# 4. Phase 3: Create Pod (Critical for File Manager/Backups/Systemd)
$PodReq = @{
    name = "test-pod-01"
    containers = @(
        @{
            name = "main"
            image = "docker.io/library/alpine:latest"
            command = @("sleep", "3600")
        }
    )
}
$Pod = Test-Endpoint -Method POST -Path "/api/pods" -Title "Create Pod" -Body $PodReq

if ($Pod) {
    $PodId = $Pod.id
    Write-Host "Created Pod ID: $PodId" -ForegroundColor Cyan

    # 5. Phase 5: File Manager
    Test-Endpoint -Method PUT -Path "/api/pods/$PodId/files/content?path=/hello.txt" -Title "Write File" -Body @{ content = "Hello NeoxAgent!" } | Out-Null
    $FileContent = Test-Endpoint -Method GET -Path "/api/pods/$PodId/files/content?path=/hello.txt" -Title "Read File"
    if ($FileContent.content -eq "Hello NeoxAgent!") {
        Write-Host "   File Content Match ✅" -ForegroundColor Green
    } else {
        Write-Host "   File Content Mismatch ❌" -ForegroundColor Red
    }
    Test-Endpoint -Method POST -Path "/api/pods/$PodId/files/create-dir?path=/testdir" -Title "Create Directory" | Out-Null
    Test-Endpoint -Method GET -Path "/api/pods/$PodId/files?path=/" -Title "List Files" | Out-Null

    # 6. Phase 6: Backups
    $Backup = Test-Endpoint -Method POST -Path "/api/pods/$PodId/backups" -Title "Create Backup" -Body @{ stop_server = $false; description = "Test Backup" }
    
    if ($Backup) {
        $BackupId = $Backup.id
        Test-Endpoint -Method GET -Path "/api/pods/$PodId/backups" -Title "List Backups" | Out-Null
        Test-Endpoint -Method DELETE -Path "/api/pods/$PodId/backups/$BackupId" -Title "Delete Backup" | Out-Null
    }

    # 7. Phase 7: Systemd Integration
    # Note: Requires root or proper user setup for systemd. Errors expected in some envs.
    # We allow failure here gracefully.
    try {
        Test-Endpoint -Method POST -Path "/api/pods/$PodId/systemd/generate" -Title "Generate Systemd Service" | Out-Null
        Test-Endpoint -Method GET -Path "/api/pods/$PodId/systemd/status" -Title "Check Systemd Status" | Out-Null
    } catch {
        Write-Host "Systemd tests skipped/failed (Env dependent)" -ForegroundColor Yellow
    }

    # Clean Up Pod
    Test-Endpoint -Method DELETE -Path "/api/pods/$PodId?force=true" -Title "Delete Pod" | Out-Null
}

# 8. Phase 7: Images
Test-Endpoint -Method GET -Path "/api/images" -Title "List Images" | Out-Null
# Search
Test-Endpoint -Method GET -Path "/api/images/search?q=alpine&limit=5" -Title "Search Images" | Out-Null

Write-Host "=========================================" -ForegroundColor Cyan
Write-Host "🎉 All Tests Completed!" -ForegroundColor Cyan
