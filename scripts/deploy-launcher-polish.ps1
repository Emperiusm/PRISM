#!/usr/bin/env pwsh
<#
.SYNOPSIS
  Creates GitHub labels and issues for the Launcher Polish phase chain.
  Populates .github/launcher-polish-sequence.json with issue numbers.

.DESCRIPTION
  Requires: gh CLI authenticated with repo access.
  Run from the repo root: .\scripts\deploy-launcher-polish.ps1

.PARAMETER DryRun
  Print commands without executing them.
#>
param(
    [switch]$DryRun
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = git rev-parse --show-toplevel
$sequencePath = Join-Path $repoRoot ".github" "launcher-polish-sequence.json"
$issueDir = Join-Path $repoRoot ".github" "issues" "launcher-polish"

# ── 1. Verify prerequisites ─────────────────────────────────────────────────
Write-Host "Checking prerequisites..." -ForegroundColor Cyan

$ghVersion = gh --version 2>&1
if ($LASTEXITCODE -ne 0) {
    Write-Error "gh CLI not found. Install from https://cli.github.com/"
}

$authStatus = gh auth status 2>&1
if ($LASTEXITCODE -ne 0) {
    Write-Error "gh CLI not authenticated. Run: gh auth login"
}

# ── 2. Create labels ────────────────────────────────────────────────────────
Write-Host "`nCreating labels..." -ForegroundColor Cyan

$labels = @(
    @{ name = "launcher-polish"; color = "0F6CBD"; description = "Launcher UI polish — automated phase chain" }
    @{ name = "pass-0"; color = "B4D6F7"; description = "Pass 0: Data Prerequisites" }
    @{ name = "pass-1"; color = "8FBFED"; description = "Pass 1: Foundations" }
    @{ name = "pass-2"; color = "6AA8E3"; description = "Pass 2: Icons & Header" }
    @{ name = "pass-3"; color = "4591D9"; description = "Pass 3: Data & Screens" }
    @{ name = "pass-4"; color = "207ACF"; description = "Pass 4: Settings & Polish" }
    @{ name = "pass-5"; color = "0F6CBD"; description = "Pass 5: Verification" }
)

# Phase labels (phase-0 through phase-11)
for ($i = 0; $i -le 11; $i++) {
    $labels += @{ name = "phase-$i"; color = "E8E8E8"; description = "Launcher polish Phase $i" }
}

foreach ($label in $labels) {
    $cmd = "gh label create `"$($label.name)`" --color `"$($label.color)`" --description `"$($label.description)`" --force"
    if ($DryRun) {
        Write-Host "  [DRY RUN] $cmd" -ForegroundColor DarkGray
    } else {
        Write-Host "  Creating label: $($label.name)"
        gh label create $label.name --color $label.color --description $label.description --force 2>&1 | Out-Null
    }
}

# ── 3. Create issues in sequence order ───────────────────────────────────────
Write-Host "`nCreating issues..." -ForegroundColor Cyan

$sequence = Get-Content $sequencePath -Raw | ConvertFrom-Json

# Phase execution order (matches sequence.json)
$phaseOrder = @(0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11)

foreach ($idx in 0..($phaseOrder.Count - 1)) {
    $phaseNum = $phaseOrder[$idx]
    $entry = $sequence | Where-Object { $_.label -eq "phase-$phaseNum" }
    $issueFile = Join-Path $issueDir "phase-$phaseNum.md"

    if (-not (Test-Path $issueFile)) {
        Write-Warning "  Issue file not found: $issueFile — skipping"
        continue
    }

    $body = Get-Content $issueFile -Raw
    $title = $entry.title
    $passLabel = "pass-$($entry.pass)"

    if ($DryRun) {
        Write-Host "  [DRY RUN] Would create issue: $title" -ForegroundColor DarkGray
    } else {
        Write-Host "  Creating: $title"
        $issueUrl = gh issue create `
            --title $title `
            --body $body `
            --label "launcher-polish" `
            --label "phase-$phaseNum" `
            --label $passLabel

        # Extract issue number from URL
        $issueNumber = ($issueUrl -split '/')[-1]
        Write-Host "    → Issue #$issueNumber" -ForegroundColor Green

        # Update sequence.json with the issue number
        $entry | Add-Member -NotePropertyName "issue" -NotePropertyValue ([int]$issueNumber) -Force
    }
}

# ── 4. Write back sequence.json with issue numbers ──────────────────────────
if (-not $DryRun) {
    Write-Host "`nUpdating sequence file with issue numbers..." -ForegroundColor Cyan
    $sequence | ConvertTo-Json -Depth 3 | Set-Content $sequencePath -Encoding UTF8
    Write-Host "  Updated: $sequencePath" -ForegroundColor Green
}

# ── 5. Enable auto-merge on the repo (requires admin) ───────────────────────
Write-Host "`nEnabling auto-merge on repo..." -ForegroundColor Cyan
if ($DryRun) {
    Write-Host "  [DRY RUN] gh repo edit --enable-auto-merge" -ForegroundColor DarkGray
} else {
    gh repo edit --enable-auto-merge 2>&1 | Out-Null
    if ($LASTEXITCODE -eq 0) {
        Write-Host "  Auto-merge enabled." -ForegroundColor Green
    } else {
        Write-Warning "  Could not enable auto-merge. Enable manually in repo Settings → General."
    }
}

# ── 6. Summary ──────────────────────────────────────────────────────────────
Write-Host "`n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━" -ForegroundColor Cyan
Write-Host "Setup complete!" -ForegroundColor Green
Write-Host ""
Write-Host "Next steps:" -ForegroundColor Yellow
Write-Host "  1. Enable Copilot Coding Agent in repo Settings → Copilot"
Write-Host "  2. Add branch protection rule for 'main':"
Write-Host "     - Require status check: 'fmt + clippy + test'"
Write-Host "     - Require branches to be up to date"
Write-Host "  3. Commit these files and push to main:"
Write-Host "     git add .github/"
Write-Host "     git commit -m 'ci: add launcher polish phase chain'"
Write-Host "     git push origin main"
Write-Host "  4. Assign Copilot to the first issue to kick off the chain:"

$firstIssue = $sequence | Where-Object { $_.label -eq "phase-0" }
if ($firstIssue.issue) {
    Write-Host "     gh issue edit $($firstIssue.issue) --add-assignee copilot" -ForegroundColor White
} else {
    Write-Host "     gh issue edit <FIRST_ISSUE_NUMBER> --add-assignee copilot" -ForegroundColor White
}

Write-Host "`n  The chain will automatically assign the next phase when each PR merges." -ForegroundColor DarkGray
Write-Host "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━" -ForegroundColor Cyan
