<#
.SYNOPSIS
    WhatsApp Agent Hub - Automated QA & Security Verification Suite (Iluminati)
.DESCRIPTION
    Executes end-to-end HTTP tests (Health, Basic Auth, Admin CRUD, Dark Mode CSS,
    Webhook filters, malformed payloads, multi-tenant routing, toggle pause) and
    validates Zero-Trust Layer 1/2 prompt injection defense via `cargo test`.
#>
param(
    [string]$BaseUrl = "http://127.0.0.1:3099",
    [string]$AdminUser = "admin",
    [string]$AdminPass = "qa_secret_2026",
    [switch]$ExternalServer
)

$ErrorActionPreference = "Stop"
$env:PATH += ";$env:USERPROFILE\.cargo\bin"

$passed = 0
$failed = 0
$serverProc = $null
$tempDb = Join-Path $PSScriptRoot "qa_test_hub.db"
$logOut = Join-Path $PSScriptRoot "qa_server.out.log"
$logErr = Join-Path $PSScriptRoot "qa_server.err.log"

function Assert-Test {
    param(
        [string]$Name,
        [bool]$Condition,
        [string]$Detail = ""
    )
    if ($Condition) {
        Write-Host "[PASS] $Name" -ForegroundColor Green
        $script:passed++
    } else {
        Write-Host "[FAIL] $Name $Detail" -ForegroundColor Red
        $script:failed++
    }
}

function Invoke-RawHttp {
    param(
        [string]$Method,
        [string]$Uri,
        [hashtable]$Headers = @{},
        [string]$Body = $null,
        [string]$ContentType = $null
    )
    $req = [System.Net.HttpWebRequest]::Create($Uri)
    $req.Method = $Method
    $req.AllowAutoRedirect = $false
    $req.Timeout = 10000
    foreach ($key in $Headers.Keys) {
        $req.Headers.Add($key, $Headers[$key])
    }
    if (-not [string]::IsNullOrEmpty($ContentType)) {
        $req.ContentType = $ContentType
    }
    if (-not [string]::IsNullOrEmpty($Body)) {
        $bytes = [System.Text.Encoding]::UTF8.GetBytes($Body)
        $req.ContentLength = $bytes.Length
        $reqStream = $req.GetRequestStream()
        $reqStream.Write($bytes, 0, $bytes.Length)
        $reqStream.Close()
    } elseif ($Method -eq "POST") {
        $req.ContentLength = 0
    }

    try {
        $resp = [System.Net.HttpWebResponse]$req.GetResponse()
        $reader = New-Object System.IO.StreamReader($resp.GetResponseStream())
        $content = $reader.ReadToEnd()
        $status = [int]$resp.StatusCode
        $resp.Close()
        return @{ Status = $status; Content = $content }
    } catch [System.Net.WebException] {
        $webResp = [System.Net.HttpWebResponse]$_.Exception.Response
        if ($null -ne $webResp) {
            $status = [int]$webResp.StatusCode
            $content = ""
            try {
                $reader = New-Object System.IO.StreamReader($webResp.GetResponseStream())
                $content = $reader.ReadToEnd()
            } catch {}
            $webResp.Close()
            return @{ Status = $status; Content = $content }
        }
        throw
    }
}

try {
    Write-Host "============================================================" -ForegroundColor Cyan
    Write-Host " WhatsApp Agent Hub - E2E & Zero-Trust QA Suite (Iluminati)" -ForegroundColor Cyan
    Write-Host "============================================================" -ForegroundColor Cyan

    # 1. Run Rust unit + integration tests (includes Zero-Trust Layer 1/2 prompt injection & tenant isolation)
    Write-Host "`n[1/2] Running Rust Integration & Zero-Trust Security Tests..." -ForegroundColor Yellow
    Push-Location $PSScriptRoot
    & cargo test --quiet
    Assert-Test "Rust integration & Zero-Trust Layer 1/2 prompt-injection suite (7 tests)" ($LASTEXITCODE -eq 0)
    Pop-Location

    # 2. Boot local binary if not targeting an external server
    if (-not $ExternalServer) {
        Remove-Item "$tempDb*",$logOut,$logErr -Force -ErrorAction SilentlyContinue
        $binPath = Join-Path $PSScriptRoot "target\release\whatsapp-agent-hub.exe"
        if (-not (Test-Path $binPath)) {
            $binPath = Join-Path $PSScriptRoot "target\debug\whatsapp-agent-hub.exe"
        }
        if (-not (Test-Path $binPath)) {
            Write-Host "Building binary for live HTTP verification..." -ForegroundColor Yellow
            Push-Location $PSScriptRoot
            & cargo build --quiet
            Pop-Location
            $binPath = Join-Path $PSScriptRoot "target\debug\whatsapp-agent-hub.exe"
        }

        $uriObj = [System.Uri]$BaseUrl
        $env:PORT = "$($uriObj.Port)"
        $env:DATABASE_URL = "sqlite:qa_test_hub.db?mode=rwc"
        $env:ADMIN_USER = $AdminUser
        $env:ADMIN_PASS = $AdminPass
        $env:EVOLUTION_API_URL = "http://127.0.0.1:9"
        $env:EVOLUTION_API_KEY = "test-key"
        $env:GEMINI_API_KEY = ""

        $serverProc = Start-Process -FilePath $binPath -WorkingDirectory $PSScriptRoot -NoNewWindow -PassThru -RedirectStandardOutput $logOut -RedirectStandardError $logErr

        # Wait for health endpoint
        $ready = $false
        for ($i = 0; $i -lt 30; $i++) {
            Start-Sleep -Milliseconds 200
            try {
                $h = Invoke-RawHttp -Method "GET" -Uri "$BaseUrl/health"
                if ($h.Status -eq 200) { $ready = $true; break }
            } catch {}
        }
        if (-not $ready) { throw "Server failed to start on $BaseUrl" }
    }

    Write-Host "`n[2/2] Running Live HTTP End-to-End Tests against $BaseUrl..." -ForegroundColor Yellow

    $validAuth = "Basic " + [Convert]::ToBase64String([Text.Encoding]::ASCII.GetBytes("${AdminUser}:${AdminPass}"))
    $badAuth   = "Basic " + [Convert]::ToBase64String([Text.Encoding]::ASCII.GetBytes("${AdminUser}:wrongpass"))

    # Test 1: Health Check
    $rHealth = Invoke-RawHttp -Method "GET" -Uri "$BaseUrl/health"
    Assert-Test "GET /health returns 200 OK" ($rHealth.Status -eq 200 -and $rHealth.Content -match '"status":"ok"')

    # Test 2: Admin without Basic Auth -> 401
    $rUnauth = Invoke-RawHttp -Method "GET" -Uri "$BaseUrl/"
    Assert-Test "GET / without credentials returns 401 Unauthorized" ($rUnauth.Status -eq 401) "Got $($rUnauth.Status)"

    # Test 3: Admin with invalid Basic Auth -> 401
    $rBadAuth = Invoke-RawHttp -Method "GET" -Uri "$BaseUrl/" -Headers @{ Authorization = $badAuth }
    Assert-Test "GET / with invalid credentials returns 401 Unauthorized" ($rBadAuth.Status -eq 401) "Got $($rBadAuth.Status)"

    # Test 4: Admin with valid Basic Auth -> 200 OK
    $rDash = Invoke-RawHttp -Method "GET" -Uri "$BaseUrl/" -Headers @{ Authorization = $validAuth }
    Assert-Test "GET / with valid Basic Auth returns 200 OK" ($rDash.Status -eq 200 -and $rDash.Content -match "WhatsApp Agent Hub")

    # Test 5: Dark Mode CSS served without external CDNs
    $rCss = Invoke-RawHttp -Method "GET" -Uri "$BaseUrl/static/style.css"
    Assert-Test "GET /static/style.css returns 200 with Dark Mode tokens" ($rCss.Status -eq 200 -and $rCss.Content -match "#0f1115" -and $rCss.Content -match "#181b21")

    # Test 6: Create Tenant A & Tenant B via Admin form
    $formA = "tenant_slug=agent-mecanica-silva&evolution_instance=inst_mecanica_silva_01&system_prompt=Assistente+da+Mecanica+Silva&tool_consultar_agenda=consultar_agenda&tool_criar_agendamento=criar_agendamento"
    $rCreateA = Invoke-RawHttp -Method "POST" -Uri "$BaseUrl/admin/agents" -Headers @{ Authorization = $validAuth } -Body $formA -ContentType "application/x-www-form-urlencoded"
    Assert-Test "POST /admin/agents creates Tenant A (303 redirect)" ($rCreateA.Status -eq 303) "Got $($rCreateA.Status)"

    $formB = "tenant_slug=agent-clinica-saude&evolution_instance=inst_clinica_saude_01&system_prompt=Assistente+da+Clinica+Saude&tool_consultar_agenda=consultar_agenda"
    $rCreateB = Invoke-RawHttp -Method "POST" -Uri "$BaseUrl/admin/agents" -Headers @{ Authorization = $validAuth } -Body $formB -ContentType "application/x-www-form-urlencoded"
    Assert-Test "POST /admin/agents creates Tenant B (303 redirect)" ($rCreateB.Status -eq 303) "Got $($rCreateB.Status)"

    $rDashAfter = Invoke-RawHttp -Method "GET" -Uri "$BaseUrl/" -Headers @{ Authorization = $validAuth }
    Assert-Test "Dashboard renders both tenants" ($rDashAfter.Content -match "agent-mecanica-silva" -and $rDashAfter.Content -match "agent-clinica-saude")

    # Test 7: Malformed JSON webhook -> 400 (no panic)
    $rBadJson = Invoke-RawHttp -Method "POST" -Uri "$BaseUrl/webhook/evolution" -Body "{broken_json" -ContentType "application/json"
    Assert-Test "POST /webhook/evolution malformed JSON handled gracefully (400)" ($rBadJson.Status -eq 400) "Got $($rBadJson.Status)"

    # Test 8: Group message ignored
    $groupPayload = '{"event":"messages.upsert","instance":"inst_mecanica_silva_01","data":{"key":{"remoteJid":"12036300000@g.us","fromMe":false},"message":{"conversation":"Oi grupo"}}}'
    $rGroup = Invoke-RawHttp -Method "POST" -Uri "$BaseUrl/webhook/evolution" -Body $groupPayload -ContentType "application/json"
    Assert-Test "Webhook ignores WhatsApp group messages (@g.us)" ($rGroup.Status -eq 200 -and $rGroup.Content -match "group_or_invalid_jid")

    # Test 9: fromMe message ignored
    $fromMePayload = '{"event":"messages.upsert","instance":"inst_mecanica_silva_01","data":{"key":{"remoteJid":"5511999990001@s.whatsapp.net","fromMe":true},"message":{"conversation":"Self message"}}}'
    $rFromMe = Invoke-RawHttp -Method "POST" -Uri "$BaseUrl/webhook/evolution" -Body $fromMePayload -ContentType "application/json"
    Assert-Test "Webhook ignores fromMe=true messages" ($rFromMe.Status -eq 200 -and $rFromMe.Content -match "from_me")

    # Test 10: Valid message routed to Tenant A
    $validA = '{"event":"messages.upsert","instance":"inst_mecanica_silva_01","data":{"key":{"remoteJid":"5511999990001@s.whatsapp.net","fromMe":false},"message":{"conversation":"Quero agendar revisao"}}}'
    $rValidA = Invoke-RawHttp -Method "POST" -Uri "$BaseUrl/webhook/evolution" -Body $validA -ContentType "application/json"
    Assert-Test "Webhook routes valid message to Tenant A" ($rValidA.Status -eq 200 -and $rValidA.Content -match '"tenant":"agent-mecanica-silva"')

    # Test 11: Valid message (extendedTextMessage) routed to Tenant B
    $validB = '{"event":"messages.upsert","instance":"inst_clinica_saude_01","data":{"key":{"remoteJid":"5511999990001@s.whatsapp.net","fromMe":false},"message":{"extendedTextMessage":{"text":"Horarios disponiveis?"}}}}'
    $rValidB = Invoke-RawHttp -Method "POST" -Uri "$BaseUrl/webhook/evolution" -Body $validB -ContentType "application/json"
    Assert-Test "Webhook routes extendedTextMessage to Tenant B" ($rValidB.Status -eq 200 -and $rValidB.Content -match '"tenant":"agent-clinica-saude"')

    # Test 12: Toggle agent pause via Admin & verify webhook ignores inactive tenant
    $rToggle = Invoke-RawHttp -Method "POST" -Uri "$BaseUrl/admin/agents/1/toggle" -Headers @{ Authorization = $validAuth }
    Assert-Test "POST /admin/agents/1/toggle pauses Tenant A (303)" ($rToggle.Status -eq 303) "Got $($rToggle.Status)"

    $rPausedTenant = Invoke-RawHttp -Method "POST" -Uri "$BaseUrl/webhook/evolution" -Body $validA -ContentType "application/json"
    Assert-Test "Webhook ignores messages for paused Tenant A" ($rPausedTenant.Status -eq 200 -and $rPausedTenant.Content -match "agent_not_found_or_inactive")

    # Final Health verify (zero panics during entire suite)
    $rHealthFinal = Invoke-RawHttp -Method "GET" -Uri "$BaseUrl/health"
    Assert-Test "Server remains healthy after full test suite" ($rHealthFinal.Status -eq 200)

} finally {
    if ($null -ne $serverProc -and -not $serverProc.HasExited) {
        $serverProc.Kill()
        $serverProc.WaitForExit(3000) | Out-Null
    }
    if (-not $ExternalServer) {
        Remove-Item "$tempDb*",$logOut,$logErr -Force -ErrorAction SilentlyContinue
    }
}

Write-Host "`n============================================================" -ForegroundColor Cyan
Write-Host " SUMMARY: $passed passed, $failed failed" -ForegroundColor $(if ($failed -eq 0) { "Green" } else { "Red" })
Write-Host "============================================================" -ForegroundColor Cyan

if ($failed -gt 0) {
    exit 1
}
