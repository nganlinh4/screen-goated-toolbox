#requires -Version 7.2

<#
.SYNOPSIS
Direct PictoFlux multi-model image automation with live catalog preflight and no Chrome dependency.

.DESCRIPTION
Pricing is intentionally documented here because the model selector shows the
text/reference-generation price, which is lower than the strict image-edit price.
The script always re-fetches the live catalog before submission; this table is a
human-readable snapshot from 2026-08-04, not a hard-coded billing authority.

One-output base prices at 1K (PictoFlux Basic is limited to 0.5K):

  Model                 Access              Generate  Strict edit  Input references
  PictoFlux Basic       anonymous or later  0         1 at 0.5K    0 generate / 4 edit
  GPT Image 2           registered free+    10        15           up to 16
  Nano Banana           Starter+            10        12           up to 4
  Nano Banana 2         Starter+            15        20           up to 12
  Nano Banana Pro       Starter+            20        24           up to 12

Current client-side charge calculation:

  totalCredits = baseCredits * max(1, outputImageCount) * resolutionMultiplier

  0.5K = 0.5x, 1K = 1x, 1.5K = 1.5x, 2K = 2x, 4K = 3x

Input-reference count is not a price multiplier. One reference and the model's
maximum number of references cost the same when model, operation, output count,
and resolution are unchanged. References remain subject to model count limits,
1.5 MB per prepared image, and 12 MB prepared aggregate size.

"Generate" may be text-only or reference-guided generation. "Strict edit"
requires a source image and should preserve/transform it more faithfully. Output
count multiplies price linearly. Free allows one output, Starter/Pro two, and
Ultra four. Never rotate visitor identities or create repeated accounts to reset
credit grants; persist one visitor identity and use an authorized account/plan.
This temporary runner deliberately submits one output per job, while the shared
credit estimator retains the full output-count formula for future integration.

Each accepted invocation gets a new, direct-child workspace under the SGT local
application-data directory. The workspace has a durable ownership marker and is
removed with exact, non-recursive cleanup on every terminal path. A bounded sweep
may remove marker-only orphans older than one hour. Workspace isolation never
rotates the persisted PictoFlux identity and therefore does not reset credits.
#>

[CmdletBinding()]
param(
    [string[]] $ImagePath = @(),

    [ValidateNotNullOrEmpty()]
    [string] $Prompt = "Turn character into a boy",

    [ValidateSet("generate", "edit")]
    [string] $Operation = "edit",

    [ValidateSet(
        "pictoflux-basic",
        "gpt-image-2",
        "gemini-2.5-flash-image",
        "gemini-3.1-flash-image-preview",
        "gemini-3-pro-image-preview"
    )]
    [Alias("Model")]
    [string] $ModelSlug = "gemini-3-pro-image-preview",

    [ValidateNotNullOrEmpty()]
    [string] $OutputDir = (Join-Path ([Environment]::GetFolderPath("UserProfile")) "Downloads"),

    [ValidateSet("auto", "1:1", "3:4", "4:3", "2:3", "3:2", "9:16", "16:9")]
    [string] $AspectRatio = "auto",

    [ValidateSet("0.5K", "1K", "1.5K", "2K", "4K")]
    [string] $Resolution = "1K",

    [ValidateRange(30, 900)]
    [int] $TimeoutSeconds = 300,

    [string] $JobId,
    [string] $VisitorId,
    [string] $SessionCookie,
    [string] $TurnstileToken,

    [switch] $ContractOnly
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$script:JobId = if ([string]::IsNullOrWhiteSpace($JobId)) {
    "pictoflux_image_$(([guid]::NewGuid().ToString("N")))"
} else {
    $JobId.Trim()
}
$script:LastProgressRatio = 0.0
$script:HttpClient = $null
$script:Cancellation = $null
$script:Workspace = $null

$PictoFluxOrigin = "https://pictoflux.com"
$SourcePath = if ($Operation -eq "edit") { "/tools/ai-image-edit" } else { "/ai-image-generator" }
$Feature = if ($Operation -eq "edit") { "image_edit" } else { "image_generate" }
$TaskMode = if ($Operation -eq "edit") { "edit" } else { "generate" }
$SelectedModelSlug = $ModelSlug
$MaxSourceBytes = 50MB
$MaxSourcePixels = 50000000
$MinSourceSide = 128
$MaxPreparedSide = 1536
$MaxPreparedBytes = 1500000
$MaxPreparedAggregateBytes = 12MB
$MaxGeneratedBytes = 64MB
$MaxGeneratedSide = 8192
$MaxGeneratedPixels = 64000000
$WorkspaceMarkerName = ".sgt-workspace.json"
$WorkspaceMarkerOwner = "screen-goated-toolbox.pictoflux-image-automation"
$WorkspaceMarkerVersion = 1
$WorkspaceOrphanGrace = [TimeSpan]::FromHours(1)
$WorkspaceSweepLimit = 128
$ResolutionMultipliers = @{
    "0.5K" = 0.5
    "1K" = 1.0
    "1.5K" = 1.5
    "2K" = 2.0
    "4K" = 3.0
}

function Write-Ndjson {
    param([Parameter(Mandatory = $true)] [object] $Value)

    $json = $Value | ConvertTo-Json -Depth 16 -Compress
    [Console]::Out.WriteLine($json)
    [Console]::Out.Flush()
}

function Write-JobProgress {
    param(
        [Parameter(Mandatory = $true)] [string] $Text,
        [Parameter(Mandatory = $true)] [string] $Stage,
        [Parameter(Mandatory = $true)] [string] $Phase,
        [Parameter(Mandatory = $true)] [double] $Ratio,
        [hashtable] $Details = @{}
    )

    $bounded = [Math]::Max($script:LastProgressRatio, [Math]::Min(0.99, $Ratio))
    $script:LastProgressRatio = $bounded
    $event = [ordered]@{
        event = "progress"
        id = $script:JobId
        stage = $Stage
        phase = $Phase
        progressRatio = [Math]::Round($bounded, 4)
        progressText = $Text
    }
    foreach ($entry in $Details.GetEnumerator()) {
        $event[$entry.Key] = $entry.Value
    }
    Write-Ndjson $event
}

function Stop-Creation {
    param(
        [Parameter(Mandatory = $true)] [string] $Code,
        [Parameter(Mandatory = $true)] [string] $Message
    )

    $exception = [InvalidOperationException]::new($Message)
    $exception.Data["errorCode"] = $Code
    throw $exception
}

function Get-EstimatedCredits {
    param(
        [Parameter(Mandatory = $true)] [double] $BaseCredits,
        [ValidateRange(1, 4)] [int] $OutputImageCount = 1,
        [Parameter(Mandatory = $true)] [string] $SelectedResolution
    )

    $multiplier = $ResolutionMultipliers[$SelectedResolution]
    if ($null -eq $multiplier) {
        Stop-Creation "creation.input_invalid" "The selected resolution has no pricing multiplier."
    }
    return $BaseCredits * [Math]::Max(1, $OutputImageCount) * [double] $multiplier
}

function Test-VisitorId {
    param([string] $Value)
    return -not [string]::IsNullOrWhiteSpace($Value) -and
        $Value.Length -le 128 -and
        $Value -match '^[a-zA-Z0-9._:-]+$'
}

function Write-DurableBytes {
    param(
        [Parameter(Mandatory = $true)] [string] $Path,
        [Parameter(Mandatory = $true)] [byte[]] $Bytes
    )

    $stream = [IO.FileStream]::new(
        $Path,
        [IO.FileMode]::CreateNew,
        [IO.FileAccess]::Write,
        [IO.FileShare]::None
    )
    try {
        $stream.Write($Bytes, 0, $Bytes.Length)
        $stream.Flush($true)
    } finally {
        $stream.Dispose()
    }
}

function Get-StableVisitorId {
    param([string] $ExplicitValue)

    $candidate = $ExplicitValue
    if ([string]::IsNullOrWhiteSpace($candidate)) {
        $candidate = [Environment]::GetEnvironmentVariable("PICTOFLUX_VISITOR_ID")
    }
    if (-not [string]::IsNullOrWhiteSpace($candidate)) {
        $candidate = $candidate.Trim()
        if (-not (Test-VisitorId $candidate)) {
            Stop-Creation "creation.input_invalid" "The supplied PictoFlux visitor ID is invalid."
        }
        return $candidate
    }

    $localData = [Environment]::GetFolderPath("LocalApplicationData")
    if ([string]::IsNullOrWhiteSpace($localData)) {
        Stop-Creation "creation.workspace_expired" "A stable PictoFlux visitor identity could not be stored."
    }
    $stateDirectory = Join-Path $localData "ScreenGoatedToolbox\PictoFlux"
    [IO.Directory]::CreateDirectory($stateDirectory) | Out-Null
    $statePath = Join-Path $stateDirectory "visitor-id.txt"
    if ([IO.File]::Exists($statePath)) {
        $stored = [IO.File]::ReadAllText($statePath).Trim()
        if (-not (Test-VisitorId $stored)) {
            Stop-Creation "creation.workspace_expired" "The stored PictoFlux visitor identity is invalid."
        }
        return $stored
    }

    $created = [guid]::NewGuid().ToString()
    $temporary = "$statePath.$([guid]::NewGuid().ToString("N")).tmp"
    try {
        Write-DurableBytes -Path $temporary -Bytes ([Text.UTF8Encoding]::new($false).GetBytes($created))
        try {
            [IO.File]::Move($temporary, $statePath)
        } catch [IO.IOException] {
            if (-not [IO.File]::Exists($statePath)) {
                throw
            }
        }
    } finally {
        if ([IO.File]::Exists($temporary)) {
            [IO.File]::Delete($temporary)
        }
    }
    return [IO.File]::ReadAllText($statePath).Trim()
}

function Read-U16BigEndian {
    param([byte[]] $Bytes, [int] $Offset)
    return (([int] $Bytes[$Offset]) -shl 8) -bor [int] $Bytes[$Offset + 1]
}

function Read-U24LittleEndian {
    param([byte[]] $Bytes, [int] $Offset)
    return [int] $Bytes[$Offset] -bor
        (([int] $Bytes[$Offset + 1]) -shl 8) -bor
        (([int] $Bytes[$Offset + 2]) -shl 16)
}

function Read-U32BigEndian {
    param([byte[]] $Bytes, [int] $Offset)
    return ([uint32] $Bytes[$Offset] -shl 24) -bor
        ([uint32] $Bytes[$Offset + 1] -shl 16) -bor
        ([uint32] $Bytes[$Offset + 2] -shl 8) -bor
        [uint32] $Bytes[$Offset + 3]
}

function Get-ImageInfo {
    param([Parameter(Mandatory = $true)] [byte[]] $Bytes)

    if ($Bytes.Length -ge 24 -and
        $Bytes[0] -eq 0x89 -and $Bytes[1] -eq 0x50 -and
        $Bytes[2] -eq 0x4e -and $Bytes[3] -eq 0x47) {
        return [pscustomobject]@{
            MimeType = "image/png"
            Extension = "png"
            Width = [int] (Read-U32BigEndian $Bytes 16)
            Height = [int] (Read-U32BigEndian $Bytes 20)
        }
    }

    if ($Bytes.Length -ge 12 -and
        $Bytes[0] -eq 0xff -and $Bytes[1] -eq 0xd8 -and $Bytes[2] -eq 0xff) {
        $sofMarkers = @(0xc0, 0xc1, 0xc2, 0xc3, 0xc5, 0xc6, 0xc7, 0xc9, 0xca, 0xcb, 0xcd, 0xce, 0xcf)
        $offset = 2
        while ($offset -lt $Bytes.Length - 8) {
            while ($offset -lt $Bytes.Length -and $Bytes[$offset] -ne 0xff) {
                $offset++
            }
            while ($offset -lt $Bytes.Length -and $Bytes[$offset] -eq 0xff) {
                $offset++
            }
            if ($offset -ge $Bytes.Length) {
                break
            }
            $marker = [int] $Bytes[$offset]
            $offset++
            if ($marker -eq 0xd8 -or $marker -eq 0xd9 -or $marker -eq 0x01 -or
                ($marker -ge 0xd0 -and $marker -le 0xd7)) {
                continue
            }
            if ($offset + 1 -ge $Bytes.Length) {
                break
            }
            $segmentLength = Read-U16BigEndian $Bytes $offset
            if ($segmentLength -lt 2 -or $offset + $segmentLength -gt $Bytes.Length) {
                break
            }
            if ($sofMarkers -contains $marker -and $segmentLength -ge 7) {
                return [pscustomobject]@{
                    MimeType = "image/jpeg"
                    Extension = "jpg"
                    Width = Read-U16BigEndian $Bytes ($offset + 5)
                    Height = Read-U16BigEndian $Bytes ($offset + 3)
                }
            }
            $offset += $segmentLength
        }
        Stop-Creation "creation.input_invalid" "The JPEG dimensions could not be read."
    }

    if ($Bytes.Length -ge 30 -and
        [Text.Encoding]::ASCII.GetString($Bytes, 0, 4) -eq "RIFF" -and
        [Text.Encoding]::ASCII.GetString($Bytes, 8, 4) -eq "WEBP") {
        $chunk = [Text.Encoding]::ASCII.GetString($Bytes, 12, 4)
        if ($chunk -eq "VP8X") {
            return [pscustomobject]@{
                MimeType = "image/webp"
                Extension = "webp"
                Width = (Read-U24LittleEndian $Bytes 24) + 1
                Height = (Read-U24LittleEndian $Bytes 27) + 1
            }
        }
        if ($chunk -eq "VP8L" -and $Bytes[20] -eq 0x2f) {
            $width = 1 + ([int] $Bytes[21] -bor (([int] $Bytes[22] -band 0x3f) -shl 8))
            $height = 1 + ((([int] $Bytes[22] -band 0xc0) -shr 6) -bor
                (([int] $Bytes[23]) -shl 2) -bor
                (([int] $Bytes[24] -band 0x0f) -shl 10))
            return [pscustomobject]@{
                MimeType = "image/webp"
                Extension = "webp"
                Width = $width
                Height = $height
            }
        }
        if ($chunk -eq "VP8 " -and
            $Bytes[23] -eq 0x9d -and $Bytes[24] -eq 0x01 -and $Bytes[25] -eq 0x2a) {
            return [pscustomobject]@{
                MimeType = "image/webp"
                Extension = "webp"
                Width = (([int] $Bytes[26] -bor (([int] $Bytes[27]) -shl 8)) -band 0x3fff)
                Height = (([int] $Bytes[28] -bor (([int] $Bytes[29]) -shl 8)) -band 0x3fff)
            }
        }
        Stop-Creation "creation.input_invalid" "The WebP dimensions could not be read."
    }

    Stop-Creation "creation.input_invalid" "The source is not a supported PNG, JPEG, or WebP image."
}

function Confirm-ImageLimits {
    param(
        [Parameter(Mandatory = $true)] [object] $Info,
        [Parameter(Mandatory = $true)] [long] $Length,
        [Parameter(Mandatory = $true)] [long] $MaximumBytes,
        [Parameter(Mandatory = $true)] [int] $MaximumSide,
        [Parameter(Mandatory = $true)] [long] $MaximumPixels,
        [int] $MinimumSide = 1
    )

    if ($Length -lt 32 -or $Length -gt $MaximumBytes) {
        Stop-Creation "creation.input_invalid" "The image exceeds the supported size limit."
    }
    if ($Info.Width -lt $MinimumSide -or $Info.Height -lt $MinimumSide -or
        $Info.Width -gt $MaximumSide -or $Info.Height -gt $MaximumSide -or
        ([long] $Info.Width * [long] $Info.Height) -gt $MaximumPixels) {
        Stop-Creation "creation.input_invalid" "The image dimensions exceed the supported limits."
    }
}

function Save-JpegToMemory {
    param(
        [Parameter(Mandatory = $true)] [Drawing.Image] $Image,
        [Parameter(Mandatory = $true)] [int] $Quality
    )

    $codec = [Drawing.Imaging.ImageCodecInfo]::GetImageEncoders() |
        Where-Object { $_.MimeType -eq "image/jpeg" } |
        Select-Object -First 1
    if ($null -eq $codec) {
        Stop-Creation "creation.failed" "The JPEG encoder is unavailable."
    }
    $qualityParameter = [Drawing.Imaging.EncoderParameter]::new(
        [Drawing.Imaging.Encoder]::Quality,
        [long] $Quality
    )
    $parameters = [Drawing.Imaging.EncoderParameters]::new(1)
    $parameters.Param[0] = $qualityParameter
    $memory = [IO.MemoryStream]::new()
    try {
        $Image.Save($memory, $codec, $parameters)
        return $memory.ToArray()
    } finally {
        $memory.Dispose()
        $parameters.Dispose()
        $qualityParameter.Dispose()
    }
}

function Prepare-ReferenceImage {
    param(
        [Parameter(Mandatory = $true)] [byte[]] $SourceBytes,
        [Parameter(Mandatory = $true)] [object] $SourceInfo
    )

    if ($SourceBytes.Length -le $MaxPreparedBytes -and
        $SourceInfo.Width -le $MaxPreparedSide -and
        $SourceInfo.Height -le $MaxPreparedSide) {
        return [pscustomobject]@{
            Bytes = $SourceBytes
            MimeType = $SourceInfo.MimeType
            Width = $SourceInfo.Width
            Height = $SourceInfo.Height
            Reencoded = $false
        }
    }
    if ($SourceInfo.MimeType -eq "image/webp") {
        Stop-Creation "creation.input_invalid" "This WebP needs preprocessing; provide a PNG or JPEG source instead."
    }

    Add-Type -AssemblyName System.Drawing
    $input = [IO.MemoryStream]::new($SourceBytes, $false)
    $source = $null
    try {
        $source = [Drawing.Image]::FromStream($input, $true, $true)
        if ($source.PropertyIdList -contains 0x0112) {
            $orientation = [int] $source.GetPropertyItem(0x0112).Value[0]
            switch ($orientation) {
                2 { $source.RotateFlip([Drawing.RotateFlipType]::RotateNoneFlipX) }
                3 { $source.RotateFlip([Drawing.RotateFlipType]::Rotate180FlipNone) }
                4 { $source.RotateFlip([Drawing.RotateFlipType]::Rotate180FlipX) }
                5 { $source.RotateFlip([Drawing.RotateFlipType]::Rotate90FlipX) }
                6 { $source.RotateFlip([Drawing.RotateFlipType]::Rotate90FlipNone) }
                7 { $source.RotateFlip([Drawing.RotateFlipType]::Rotate270FlipX) }
                8 { $source.RotateFlip([Drawing.RotateFlipType]::Rotate270FlipNone) }
            }
        }

        $scale = [Math]::Min(1.0, [Math]::Min(
            $MaxPreparedSide / [double] $source.Width,
            $MaxPreparedSide / [double] $source.Height
        ))
        $width = [Math]::Max($MinSourceSide, [int] [Math]::Round($source.Width * $scale))
        $height = [Math]::Max($MinSourceSide, [int] [Math]::Round($source.Height * $scale))
        $qualities = @(85, 80, 75, 72)

        while ($width -ge $MinSourceSide -and $height -ge $MinSourceSide) {
            $bitmap = [Drawing.Bitmap]::new($width, $height, [Drawing.Imaging.PixelFormat]::Format24bppRgb)
            try {
                $graphics = [Drawing.Graphics]::FromImage($bitmap)
                try {
                    $graphics.Clear([Drawing.Color]::White)
                    $graphics.CompositingQuality = [Drawing.Drawing2D.CompositingQuality]::HighQuality
                    $graphics.InterpolationMode = [Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
                    $graphics.PixelOffsetMode = [Drawing.Drawing2D.PixelOffsetMode]::HighQuality
                    $graphics.SmoothingMode = [Drawing.Drawing2D.SmoothingMode]::HighQuality
                    $destination = [Drawing.Rectangle]::new(0, 0, $width, $height)
                    $graphics.DrawImage($source, $destination)
                } finally {
                    $graphics.Dispose()
                }

                foreach ($quality in $qualities) {
                    [byte[]] $candidate = Save-JpegToMemory -Image $bitmap -Quality $quality
                    if ($candidate.Length -le $MaxPreparedBytes) {
                        return [pscustomobject]@{
                            Bytes = $candidate
                            MimeType = "image/jpeg"
                            Width = $width
                            Height = $height
                            Reencoded = $true
                        }
                    }
                }
            } finally {
                $bitmap.Dispose()
            }

            $nextWidth = [int] [Math]::Floor($width * 0.85)
            $nextHeight = [int] [Math]::Floor($height * 0.85)
            if ($nextWidth -ge $width -or $nextHeight -ge $height) {
                break
            }
            $width = $nextWidth
            $height = $nextHeight
        }
    } catch [InvalidOperationException] {
        throw
    } catch {
        Stop-Creation "creation.input_invalid" "The source image could not be decoded for preprocessing."
    } finally {
        if ($null -ne $source) {
            $source.Dispose()
        }
        $input.Dispose()
    }

    Stop-Creation "creation.input_invalid" "The source image could not be prepared under PictoFlux limits."
}

function Resolve-AspectRatio {
    param(
        [string] $Requested,
        [int] $Width,
        [int] $Height
    )

    if ($Requested -ne "auto") {
        return $Requested
    }
    $ratios = [ordered]@{
        "1:1" = 1.0
        "3:4" = 0.75
        "4:3" = 4.0 / 3.0
        "2:3" = 2.0 / 3.0
        "3:2" = 1.5
        "9:16" = 9.0 / 16.0
        "16:9" = 16.0 / 9.0
    }
    $actual = $Width / [double] $Height
    $best = $null
    $bestDistance = [double]::PositiveInfinity
    foreach ($entry in $ratios.GetEnumerator()) {
        $distance = [Math]::Abs([Math]::Log($actual / [double] $entry.Value))
        if ($distance -lt $bestDistance) {
            $best = $entry.Key
            $bestDistance = $distance
        }
    }
    return $best
}

function New-PictoFluxClient {
    param(
        [Parameter(Mandatory = $true)] [string] $StableVisitorId,
        [string] $Cookie
    )

    Add-Type -AssemblyName System.Net.Http
    $handler = [Net.Http.HttpClientHandler]::new()
    $handler.AutomaticDecompression =
        [Net.DecompressionMethods]::GZip -bor
        [Net.DecompressionMethods]::Deflate -bor
        [Net.DecompressionMethods]::Brotli
    $client = [Net.Http.HttpClient]::new($handler, $true)
    $client.Timeout = [Threading.Timeout]::InfiniteTimeSpan
    $client.DefaultRequestHeaders.TryAddWithoutValidation("User-Agent", "ScreenGoatedToolbox-PictoFlux-HTTP/0.1") | Out-Null
    $client.DefaultRequestHeaders.TryAddWithoutValidation("Origin", $PictoFluxOrigin) | Out-Null
    $client.DefaultRequestHeaders.Referrer = [uri] "$PictoFluxOrigin$SourcePath"
    $client.DefaultRequestHeaders.TryAddWithoutValidation("x-visitor-id", $StableVisitorId) | Out-Null
    $offsetMinutes = [int] [TimeZoneInfo]::Local.GetUtcOffset([DateTimeOffset]::UtcNow).TotalMinutes
    $client.DefaultRequestHeaders.TryAddWithoutValidation("x-client-tz-offset", $offsetMinutes.ToString()) | Out-Null
    $client.DefaultRequestHeaders.TryAddWithoutValidation("X-Client-Timezone", "Asia/Seoul") | Out-Null
    if (-not [string]::IsNullOrWhiteSpace($Cookie)) {
        $client.DefaultRequestHeaders.TryAddWithoutValidation("Cookie", $Cookie.Trim()) | Out-Null
    }
    return $client
}

function Get-PropertyValue {
    param(
        [object] $InputObject,
        [Parameter(Mandatory = $true)] [string] $Name
    )

    if ($null -eq $InputObject) {
        return $null
    }
    $property = $InputObject.PSObject.Properties[$Name]
    if ($null -eq $property) {
        return $null
    }
    return $property.Value
}

function Get-ProviderErrorCode {
    param([string] $Body)

    if ([string]::IsNullOrWhiteSpace($Body)) {
        return $null
    }
    try {
        $parsed = $Body | ConvertFrom-Json
        $errorObject = Get-PropertyValue $parsed "error"
        $jsonObject = Get-PropertyValue $parsed "json"
        $jsonErrorObject = Get-PropertyValue $jsonObject "error"
        $candidates = @(
            (Get-PropertyValue $errorObject "code")
            (Get-PropertyValue $parsed "code")
            (Get-PropertyValue $jsonErrorObject "code")
            (Get-PropertyValue $jsonObject "code")
        )
        foreach ($candidate in $candidates) {
            if (-not [string]::IsNullOrWhiteSpace([string] $candidate)) {
                return [string] $candidate
            }
        }
    } catch {
        return $null
    }
    return $null
}

function Stop-ProviderFailure {
    param(
        [int] $StatusCode,
        [string] $ProviderCode,
        [string] $Body = ""
    )

    $normalized = ([string] $ProviderCode).ToUpperInvariant()
    if ($normalized -match "CREDIT|QUOTA|CAPACITY" -or $StatusCode -eq 429) {
        Stop-Creation "creation.capacity_unavailable" "PictoFlux does not have enough available credits or capacity for this request."
    }
    if ($normalized -match "TURNSTILE|CHALLENGE|CAPTCHA" -or $Body -match "(?i)turnstile") {
        Stop-Creation "creation.challenge_required" "PictoFlux requires a valid Turnstile token; the automation does not bypass challenges."
    }
    if ($normalized -match "UNAUTHORIZED|TOKEN|AUTH|PREMIUM|MODEL.*AVAILABLE|PLAN" -or
        $StatusCode -eq 401 -or $StatusCode -eq 403) {
        Stop-Creation "creation.workspace_expired" "The selected model requires an authorized PictoFlux session or eligible plan."
    }
    if ($StatusCode -ge 500) {
        Stop-Creation "creation.capacity_unavailable" "PictoFlux is temporarily unavailable."
    }
    Stop-Creation "creation.failed" "PictoFlux rejected the image request."
}

function Invoke-JsonPost {
    param(
        [Parameter(Mandatory = $true)] [Net.Http.HttpClient] $Client,
        [Parameter(Mandatory = $true)] [string] $Uri,
        [Parameter(Mandatory = $true)] [object] $Body,
        [Parameter(Mandatory = $true)] [Threading.CancellationToken] $CancellationToken
    )

    $json = $Body | ConvertTo-Json -Depth 12 -Compress
    $request = [Net.Http.HttpRequestMessage]::new([Net.Http.HttpMethod]::Post, $Uri)
    $request.Content = [Net.Http.StringContent]::new($json, [Text.Encoding]::UTF8, "application/json")
    try {
        $response = $Client.SendAsync(
            $request,
            [Net.Http.HttpCompletionOption]::ResponseContentRead,
            $CancellationToken
        ).GetAwaiter().GetResult()
        try {
            $content = $response.Content.ReadAsStringAsync($CancellationToken).GetAwaiter().GetResult()
            if (-not $response.IsSuccessStatusCode) {
                Stop-ProviderFailure -StatusCode ([int] $response.StatusCode) -ProviderCode (Get-ProviderErrorCode $content) -Body $content
            }
            return $content | ConvertFrom-Json
        } finally {
            $response.Dispose()
        }
    } finally {
        $request.Dispose()
    }
}

function Get-SelectedModel {
    param(
        [Parameter(Mandatory = $true)] [Net.Http.HttpClient] $Client,
        [Parameter(Mandatory = $true)] [bool] $HasSession,
        [Parameter(Mandatory = $true)] [Threading.CancellationToken] $CancellationToken
    )

    $catalogBody = [ordered]@{
        json = [ordered]@{
            feature = $Feature
            sourcePath = $SourcePath
            hasUser = $HasSession
        }
    }
    $catalogRequest = @{
        Client = $Client
        Uri = "$PictoFluxOrigin/api/rpc/ai/models/catalog"
        Body = $catalogBody
        CancellationToken = $CancellationToken
    }
    $catalog = Invoke-JsonPost @catalogRequest
    if ($catalog.json.success -ne $true -or $null -eq $catalog.json.data.models) {
        Stop-Creation "creation.failed" "PictoFlux returned an invalid model catalog."
    }
    $model = $catalog.json.data.models |
        Where-Object { $_.publicSlug -eq $SelectedModelSlug } |
        Select-Object -First 1
    if ($null -eq $model -or $model.isActive -ne $true) {
        Stop-Creation "creation.failed" "The selected model is absent from the active PictoFlux catalog."
    }
    return $model
}

function Get-BillingSummary {
    param(
        [Parameter(Mandatory = $true)] [Net.Http.HttpClient] $Client,
        [Parameter(Mandatory = $true)] [Threading.CancellationToken] $CancellationToken
    )

    $billingRequest = @{
        Client = $Client
        Uri = "$PictoFluxOrigin/api/rpc/billing/getBillingSummary"
        Body = [ordered]@{ json = [ordered]@{ organizationId = $null } }
        CancellationToken = $CancellationToken
    }
    $summary = Invoke-JsonPost @billingRequest
    if ($summary.json.success -ne $true -or $null -eq $summary.json.data) {
        Stop-Creation "creation.failed" "PictoFlux returned an invalid billing summary."
    }
    return $summary.json.data
}

function New-GenerationBody {
    param(
        [Parameter(Mandatory = $true)] [object] $Model,
        [object[]] $PreparedReferences = @(),
        [Parameter(Mandatory = $true)] [string] $ResolvedAspectRatio,
        [string] $ChallengeToken
    )

    $inputImages = @(
        foreach ($prepared in $PreparedReferences) {
            "data:$($prepared.MimeType);base64,$([Convert]::ToBase64String($prepared.Bytes))"
        }
    )
    $body = [ordered]@{
        feature = $Feature
        prompt = $Prompt.Trim()
        modelId = $Model.id
        aspectRatio = $ResolvedAspectRatio
        resolution = $Resolution
        imageCount = 1
        sourcePath = $SourcePath
        safe = $true
        inputImages = $inputImages
        taskMode = $TaskMode
    }
    if (-not [string]::IsNullOrWhiteSpace($ChallengeToken)) {
        $body.turnstileToken = $ChallengeToken.Trim()
    }
    return $body
}

function Invoke-Generation {
    param(
        [Parameter(Mandatory = $true)] [Net.Http.HttpClient] $Client,
        [Parameter(Mandatory = $true)] [object] $Body,
        [Parameter(Mandatory = $true)] [Threading.CancellationToken] $CancellationToken
    )

    $json = $Body | ConvertTo-Json -Depth 8 -Compress
    $request = [Net.Http.HttpRequestMessage]::new(
        [Net.Http.HttpMethod]::Post,
        "$PictoFluxOrigin/api/ai/generate-image"
    )
    $request.Headers.Accept.ParseAdd("text/event-stream")
    $request.Content = [Net.Http.StringContent]::new($json, [Text.Encoding]::UTF8, "application/json")
    $response = $null
    try {
        Write-JobProgress "Submitting image edit" "generating" "submitting" 0.45
        $response = $Client.SendAsync(
            $request,
            [Net.Http.HttpCompletionOption]::ResponseHeadersRead,
            $CancellationToken
        ).GetAwaiter().GetResult()
        if (-not $response.IsSuccessStatusCode) {
            $content = $response.Content.ReadAsStringAsync($CancellationToken).GetAwaiter().GetResult()
            Stop-ProviderFailure -StatusCode ([int] $response.StatusCode) -ProviderCode (Get-ProviderErrorCode $content) -Body $content
        }
        if ($null -eq $response.Content) {
            Stop-Creation "creation.failed" "PictoFlux returned an empty generation stream."
        }

        $stream = $response.Content.ReadAsStreamAsync($CancellationToken).GetAwaiter().GetResult()
        $reader = [IO.StreamReader]::new($stream, [Text.UTF8Encoding]::new($false), $true, 8192, $false)
        $dataLines = [Collections.Generic.List[string]]::new()
        $imageReference = $null
        try {
            while ($true) {
                $line = $reader.ReadLineAsync().WaitAsync($CancellationToken).GetAwaiter().GetResult()
                if ($null -eq $line) {
                    break
                }
                if ($line.Length -gt 0) {
                    if ($line.StartsWith("data:")) {
                        $dataLines.Add($line.Substring(5).TrimStart())
                    }
                    continue
                }
                if ($dataLines.Count -eq 0) {
                    continue
                }

                $payload = [string]::Join("`n", $dataLines)
                $dataLines.Clear()
                try {
                    $eventData = $payload | ConvertFrom-Json
                } catch {
                    continue
                }
                $eventType = [string] (Get-PropertyValue $eventData "type")
                switch ($eventType) {
                    "start" {
                        Write-JobProgress "Generation accepted" "generating" "accepted" 0.5
                    }
                    "queued" {
                        $details = @{}
                        $delayMs = Get-PropertyValue $eventData "delayMs"
                        if ($null -ne $delayMs) {
                            $details.queueDelayMs = [long] $delayMs
                        }
                        Write-JobProgress "Waiting for generation capacity" "generating" "provider_queue" 0.54 $details
                    }
                    "processing" {
                        Write-JobProgress "Creating image" "generating" "processing" 0.64
                    }
                    "success" {
                        $eventImage = Get-PropertyValue $eventData "image"
                        if (-not [string]::IsNullOrWhiteSpace([string] $eventImage)) {
                            $imageReference = [string] $eventImage
                            Write-JobProgress "Image generated" "finalizing" "artifact_available" 0.82
                        }
                    }
                    "error" {
                        Stop-ProviderFailure -StatusCode 200 -ProviderCode ([string] (Get-PropertyValue $eventData "code"))
                    }
                    "done" {
                        if ($null -ne $imageReference) {
                            break
                        }
                    }
                }
                if ($null -ne $imageReference -and $eventType -eq "done") {
                    break
                }
            }
        } finally {
            $reader.Dispose()
            $stream.Dispose()
        }
        if ([string]::IsNullOrWhiteSpace($imageReference)) {
            Stop-Creation "creation.failed" "PictoFlux completed without an image artifact."
        }
        return $imageReference
    } finally {
        if ($null -ne $response) {
            $response.Dispose()
        }
        $request.Dispose()
    }
}

function Get-GeneratedBytes {
    param(
        [Parameter(Mandatory = $true)] [Net.Http.HttpClient] $Client,
        [Parameter(Mandatory = $true)] [string] $Reference,
        [Parameter(Mandatory = $true)] [Threading.CancellationToken] $CancellationToken
    )

    if ($Reference.StartsWith("data:")) {
        $match = [regex]::Match(
            $Reference,
            '^data:(image/(?:png|jpeg|webp));base64,(?<data>.+)$',
            [Text.RegularExpressions.RegexOptions]::Singleline
        )
        if (-not $match.Success) {
            Stop-Creation "creation.failed" "PictoFlux returned an unsupported data URL artifact."
        }
        try {
            return [Convert]::FromBase64String($match.Groups["data"].Value)
        } catch {
            Stop-Creation "creation.failed" "PictoFlux returned a corrupt data URL artifact."
        }
    }

    $artifactUri = $null
    if (-not [uri]::TryCreate($Reference, [UriKind]::Absolute, [ref] $artifactUri) -or
        $artifactUri.Scheme -ne "https") {
        Stop-Creation "creation.failed" "PictoFlux returned an invalid artifact URL."
    }

    for ($attempt = 1; $attempt -le 2; $attempt++) {
        $request = [Net.Http.HttpRequestMessage]::new([Net.Http.HttpMethod]::Get, $artifactUri)
        $response = $null
        try {
            $response = $Client.SendAsync(
                $request,
                [Net.Http.HttpCompletionOption]::ResponseContentRead,
                $CancellationToken
            ).GetAwaiter().GetResult()
            if (-not $response.IsSuccessStatusCode) {
                if ($attempt -eq 2) {
                    Stop-ProviderFailure -StatusCode ([int] $response.StatusCode) -ProviderCode $null
                }
                Start-Sleep -Milliseconds 250
                continue
            }
            if ($response.Content.Headers.ContentLength -gt $MaxGeneratedBytes) {
                Stop-Creation "creation.failed" "The generated image exceeds the artifact size limit."
            }
            return $response.Content.ReadAsByteArrayAsync($CancellationToken).GetAwaiter().GetResult()
        } catch [InvalidOperationException] {
            throw
        } catch {
            if ($attempt -eq 2) {
                Stop-Creation "creation.failed" "The generated image could not be downloaded."
            }
            Start-Sleep -Milliseconds 250
        } finally {
            if ($null -ne $response) {
                $response.Dispose()
            }
            $request.Dispose()
        }
    }
    Stop-Creation "creation.failed" "The generated image could not be downloaded."
}

function Normalize-GeneratedImage {
    param(
        [Parameter(Mandatory = $true)] [byte[]] $Bytes,
        [Parameter(Mandatory = $true)] [object] $Info
    )

    if ($Info.MimeType -ne "image/jpeg") {
        return [pscustomobject]@{
            Bytes = $Bytes
            MimeType = $Info.MimeType
            Extension = $Info.Extension
            Width = $Info.Width
            Height = $Info.Height
        }
    }

    Add-Type -AssemblyName System.Drawing
    $input = [IO.MemoryStream]::new($Bytes, $false)
    $image = $null
    $output = [IO.MemoryStream]::new()
    try {
        $image = [Drawing.Image]::FromStream($input, $true, $true)
        $image.Save($output, [Drawing.Imaging.ImageFormat]::Png)
        [byte[]] $normalized = $output.ToArray()
        if ($normalized.Length -gt $MaxGeneratedBytes) {
            Stop-Creation "creation.failed" "The normalized image exceeds the artifact size limit."
        }
        return [pscustomobject]@{
            Bytes = $normalized
            MimeType = "image/png"
            Extension = "png"
            Width = $image.Width
            Height = $image.Height
        }
    } finally {
        if ($null -ne $image) {
            $image.Dispose()
        }
        $output.Dispose()
        $input.Dispose()
    }
}

function Get-ByteHash {
    param([Parameter(Mandatory = $true)] [byte[]] $Bytes)

    $sha = [Security.Cryptography.SHA256]::Create()
    try {
        return [Convert]::ToHexString($sha.ComputeHash($Bytes))
    } finally {
        $sha.Dispose()
    }
}

function Get-NormalizedDirectoryPath {
    param([Parameter(Mandatory = $true)] [string] $Path)

    return [IO.Path]::GetFullPath($Path).TrimEnd(
        [IO.Path]::DirectorySeparatorChar,
        [IO.Path]::AltDirectorySeparatorChar
    )
}

function Test-ReparsePoint {
    param([Parameter(Mandatory = $true)] [string] $Path)

    $attributes = [IO.File]::GetAttributes($Path)
    return ($attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0
}

function Test-DirectWorkspaceChild {
    param(
        [Parameter(Mandatory = $true)] [string] $Root,
        [Parameter(Mandatory = $true)] [string] $Candidate
    )

    $normalizedRoot = Get-NormalizedDirectoryPath $Root
    $normalizedCandidate = Get-NormalizedDirectoryPath $Candidate
    $parent = Get-NormalizedDirectoryPath ([IO.Path]::GetDirectoryName($normalizedCandidate))
    return [string]::Equals($normalizedRoot, $parent, [StringComparison]::OrdinalIgnoreCase)
}

function Get-JobWorkspaceRoot {
    $localData = [Environment]::GetFolderPath("LocalApplicationData")
    if ([string]::IsNullOrWhiteSpace($localData)) {
        Stop-Creation "creation.workspace_expired" "A local job-workspace root is unavailable."
    }

    $root = Get-NormalizedDirectoryPath (Join-Path $localData "ScreenGoatedToolbox\PictoFlux\job-workspaces")
    [IO.Directory]::CreateDirectory($root) | Out-Null
    if (Test-ReparsePoint $root) {
        Stop-Creation "creation.workspace_expired" "The job-workspace root cannot be a reparse point."
    }
    return $root
}

function Read-OwnedWorkspaceMarker {
    param(
        [Parameter(Mandatory = $true)] [string] $Root,
        [Parameter(Mandatory = $true)] [string] $WorkspacePath
    )

    if (-not [IO.Directory]::Exists($WorkspacePath) -or
        -not (Test-DirectWorkspaceChild -Root $Root -Candidate $WorkspacePath) -or
        (Test-ReparsePoint $WorkspacePath)) {
        return $null
    }

    $markerPath = [IO.Path]::GetFullPath((Join-Path $WorkspacePath $WorkspaceMarkerName))
    if (-not [IO.File]::Exists($markerPath) -or (Test-ReparsePoint $markerPath)) {
        return $null
    }
    $markerFile = [IO.FileInfo]::new($markerPath)
    if ($markerFile.Length -le 0 -or $markerFile.Length -gt 8192) {
        return $null
    }

    try {
        $marker = [IO.File]::ReadAllText($markerPath) | ConvertFrom-Json
    } catch {
        return $null
    }
    if ([int] $marker.version -ne $WorkspaceMarkerVersion -or
        [string] $marker.owner -ne $WorkspaceMarkerOwner -or
        [string]::IsNullOrWhiteSpace([string] $marker.jobId) -or
        [string]::IsNullOrWhiteSpace([string] $marker.workspaceId) -or
        [string]::IsNullOrWhiteSpace([string] $marker.identityFingerprint) -or
        -not [string]::Equals(
            [IO.Path]::GetFileName((Get-NormalizedDirectoryPath $WorkspacePath)),
            [string] $marker.workspaceId,
            [StringComparison]::OrdinalIgnoreCase
        )) {
        return $null
    }
    return $marker
}

function Remove-OwnedJobWorkspace {
    param(
        [Parameter(Mandatory = $true)] [string] $Root,
        [Parameter(Mandatory = $true)] [string] $WorkspacePath,
        [Parameter(Mandatory = $true)] [string] $ExpectedJobId,
        [Parameter(Mandatory = $true)] [string] $ExpectedWorkspaceId,
        [Parameter(Mandatory = $true)] [string] $ExpectedIdentityFingerprint
    )

    try {
        $marker = Read-OwnedWorkspaceMarker -Root $Root -WorkspacePath $WorkspacePath
        if ($null -eq $marker -or
            [string] $marker.jobId -ne $ExpectedJobId -or
            [string] $marker.workspaceId -ne $ExpectedWorkspaceId -or
            [string] $marker.identityFingerprint -ne $ExpectedIdentityFingerprint) {
            return $false
        }

        $entries = @([IO.Directory]::EnumerateFileSystemEntries($WorkspacePath))
        if ($entries.Count -ne 1 -or
            -not [string]::Equals(
                [IO.Path]::GetFileName($entries[0]),
                $WorkspaceMarkerName,
                [StringComparison]::Ordinal
            )) {
            return $false
        }

        $markerPath = [IO.Path]::GetFullPath($entries[0])
        if (Test-ReparsePoint $markerPath) {
            return $false
        }
        [IO.File]::Delete($markerPath)
        [IO.Directory]::Delete($WorkspacePath, $false)
        return -not [IO.Directory]::Exists($WorkspacePath)
    } catch {
        return $false
    }
}

function Remove-StaleJobWorkspaces {
    param([Parameter(Mandatory = $true)] [string] $Root)

    $removed = 0
    $now = [DateTime]::UtcNow
    try {
        $candidates = @(
            [IO.Directory]::EnumerateDirectories($Root) |
                Sort-Object |
                Select-Object -First $WorkspaceSweepLimit
        )
    } catch {
        return 0
    }

    foreach ($candidate in $candidates) {
        try {
            $marker = Read-OwnedWorkspaceMarker -Root $Root -WorkspacePath $candidate
            if ($null -eq $marker) {
                continue
            }
            $createdAt = [DateTime]::MinValue
            if (-not [DateTime]::TryParse(
                [string] $marker.createdAtUtc,
                [Globalization.CultureInfo]::InvariantCulture,
                [Globalization.DateTimeStyles]::AssumeUniversal -bor [Globalization.DateTimeStyles]::AdjustToUniversal,
                [ref] $createdAt
            )) {
                continue
            }
            if (($now - $createdAt) -lt $WorkspaceOrphanGrace) {
                continue
            }
            $cleanup = @{
                Root = $Root
                WorkspacePath = $candidate
                ExpectedJobId = [string] $marker.jobId
                ExpectedWorkspaceId = [string] $marker.workspaceId
                ExpectedIdentityFingerprint = [string] $marker.identityFingerprint
            }
            if (Remove-OwnedJobWorkspace @cleanup) {
                $removed++
            }
        } catch {
            continue
        }
    }
    return $removed
}

function New-JobWorkspace {
    param(
        [Parameter(Mandatory = $true)] [string] $StableVisitorId,
        [Parameter(Mandatory = $true)] [string] $CurrentJobId
    )

    $root = Get-JobWorkspaceRoot
    $staleRemoved = Remove-StaleJobWorkspaces -Root $root
    $safeJobId = ($CurrentJobId -replace '[^a-zA-Z0-9._-]', '_')
    if ($safeJobId.Length -gt 80) {
        $safeJobId = $safeJobId.Substring(0, 80)
    }
    $workspaceId = "$safeJobId-$([guid]::NewGuid().ToString("N"))"
    $workspacePath = Get-NormalizedDirectoryPath (Join-Path $root $workspaceId)
    if (-not (Test-DirectWorkspaceChild -Root $root -Candidate $workspacePath) -or
        [IO.Directory]::Exists($workspacePath) -or
        [IO.File]::Exists($workspacePath)) {
        Stop-Creation "creation.workspace_expired" "A unique isolated job workspace could not be allocated."
    }

    $identityBytes = [Text.UTF8Encoding]::new($false).GetBytes("pictoflux-visitor:$StableVisitorId")
    $identityFingerprint = (Get-ByteHash $identityBytes).Substring(0, 16).ToLowerInvariant()
    $markerPath = Join-Path $workspacePath $WorkspaceMarkerName
    $markerWritten = $false
    [IO.Directory]::CreateDirectory($workspacePath) | Out-Null
    try {
        if (Test-ReparsePoint $workspacePath) {
            Stop-Creation "creation.workspace_expired" "The isolated job workspace cannot be a reparse point."
        }
        $marker = [ordered]@{
            version = $WorkspaceMarkerVersion
            owner = $WorkspaceMarkerOwner
            jobId = $CurrentJobId
            workspaceId = $workspaceId
            identityFingerprint = $identityFingerprint
            createdAtUtc = [DateTime]::UtcNow.ToString("O")
        }
        $markerBytes = [Text.UTF8Encoding]::new($false).GetBytes(
            ($marker | ConvertTo-Json -Depth 4 -Compress)
        )
        Write-DurableBytes -Path $markerPath -Bytes $markerBytes
        $markerWritten = $true
        return [pscustomobject]@{
            Root = $root
            Path = $workspacePath
            MarkerPath = $markerPath
            JobId = $CurrentJobId
            Id = $workspaceId
            IdentityFingerprint = $identityFingerprint
            StaleWorkspacesRemoved = $staleRemoved
        }
    } catch {
        if ($markerWritten -and [IO.File]::Exists($markerPath) -and -not (Test-ReparsePoint $markerPath)) {
            [IO.File]::Delete($markerPath)
        }
        if ([IO.Directory]::Exists($workspacePath) -and
            -not (Test-ReparsePoint $workspacePath) -and
            @([IO.Directory]::EnumerateFileSystemEntries($workspacePath)).Count -eq 0) {
            [IO.Directory]::Delete($workspacePath, $false)
        }
        throw
    }
}

function Publish-Artifact {
    param(
        [Parameter(Mandatory = $true)] [string] $Directory,
        [Parameter(Mandatory = $true)] [object] $Artifact
    )

    $resolvedOutput = [IO.Path]::GetFullPath($Directory)
    [IO.Directory]::CreateDirectory($resolvedOutput) | Out-Null
    $stagingDirectory = [IO.Path]::GetFullPath((Join-Path $resolvedOutput ".sgt-creation-staging"))
    $outputPrefix = $resolvedOutput.TrimEnd([IO.Path]::DirectorySeparatorChar) + [IO.Path]::DirectorySeparatorChar
    if (-not $stagingDirectory.StartsWith($outputPrefix, [StringComparison]::OrdinalIgnoreCase)) {
        Stop-Creation "creation.input_invalid" "The artifact staging path escaped the requested output directory."
    }
    [IO.Directory]::CreateDirectory($stagingDirectory) | Out-Null

    $safeJobId = $script:JobId -replace '[^a-zA-Z0-9._-]', '_'
    $stagingPath = Join-Path $stagingDirectory "$safeJobId.$([guid]::NewGuid().ToString("N")).tmp"
    $stamp = [DateTime]::UtcNow.ToString("yyyyMMdd-HHmmssfff")
    $safeModelSlug = $SelectedModelSlug -replace '[^a-zA-Z0-9._-]', '_'
    $targetPath = Join-Path $resolvedOutput "pictoflux-$safeModelSlug-$Operation-$stamp.$($Artifact.Extension)"
    if ([IO.File]::Exists($targetPath)) {
        $targetPath = Join-Path $resolvedOutput "pictoflux-$safeModelSlug-$Operation-$stamp-$(([guid]::NewGuid().ToString("N")).Substring(0, 8)).$($Artifact.Extension)"
    }

    try {
        Write-DurableBytes -Path $stagingPath -Bytes $Artifact.Bytes
        $expectedHash = Get-ByteHash $Artifact.Bytes
        $staged = [IO.FileInfo]::new($stagingPath)
        if (-not $staged.Exists -or $staged.Length -ne $Artifact.Bytes.Length) {
            Stop-Creation "creation.failed" "The staged image failed size verification."
        }
        [IO.File]::Move($stagingPath, $targetPath)
        $published = [IO.FileInfo]::new($targetPath)
        if (-not $published.Exists -or $published.Length -ne $Artifact.Bytes.Length) {
            Stop-Creation "creation.failed" "The published image failed size verification."
        }
        $publishedHash = (Get-FileHash -LiteralPath $targetPath -Algorithm SHA256).Hash.ToUpperInvariant()
        if ($publishedHash -ne $expectedHash) {
            Stop-Creation "creation.failed" "The published image failed hash verification."
        }
        return [pscustomobject]@{
            Path = $targetPath
            SizeBytes = $published.Length
            Sha256 = $publishedHash
        }
    } finally {
        if ([IO.File]::Exists($stagingPath)) {
            [IO.File]::Delete($stagingPath)
        }
    }
}

try {
    if ([string]::IsNullOrWhiteSpace($script:JobId) -or $script:JobId.Length -gt 160) {
        Stop-Creation "creation.input_invalid" "The job ID is invalid."
    }
    $trimmedPrompt = $Prompt.Trim()
    if ($trimmedPrompt.Length -eq 0 -or $trimmedPrompt.Length -gt 4000) {
        Stop-Creation "creation.input_invalid" "The prompt is empty or too long."
    }
    $requestedImagePaths = @(
        $ImagePath |
            Where-Object { -not [string]::IsNullOrWhiteSpace($_) } |
            ForEach-Object { [IO.Path]::GetFullPath($_) }
    )
    if ($Operation -eq "edit" -and $requestedImagePaths.Count -eq 0) {
        Stop-Creation "creation.input_invalid" "Strict image editing requires at least one source image."
    }

    Write-JobProgress "Validating image request" "preparing" "validation" 0.02
    $sourceRecords = @(
        foreach ($resolvedImagePath in $requestedImagePaths) {
            if (-not [IO.File]::Exists($resolvedImagePath)) {
                Stop-Creation "creation.input_invalid" "A source image does not exist."
            }
            $sourceFile = [IO.FileInfo]::new($resolvedImagePath)
            if ($sourceFile.Length -gt $MaxSourceBytes) {
                Stop-Creation "creation.input_invalid" "A source image exceeds PictoFlux's 50 MB limit."
            }
            [byte[]] $bytes = [IO.File]::ReadAllBytes($resolvedImagePath)
            $info = Get-ImageInfo $bytes
            $sourceLimitCheck = @{
                Info = $info
                Length = $bytes.Length
                MaximumBytes = $MaxSourceBytes
                MaximumSide = 50000
                MaximumPixels = $MaxSourcePixels
                MinimumSide = $MinSourceSide
            }
            Confirm-ImageLimits @sourceLimitCheck
            [pscustomobject]@{
                Path = $resolvedImagePath
                Bytes = $bytes
                Info = $info
            }
        }
    )
    if ($sourceRecords.Count -ne $requestedImagePaths.Count) {
        Stop-Creation "creation.input_invalid" "One or more source images could not be validated."
    }

    $stableVisitorId = Get-StableVisitorId $VisitorId
    $script:Workspace = New-JobWorkspace -StableVisitorId $stableVisitorId -CurrentJobId $script:JobId
    Write-JobProgress "Fresh isolated job workspace ready" "preparing" "workspace_ready" 0.05 @{
        workspaceId = $script:Workspace.Id
        identityFingerprint = $script:Workspace.IdentityFingerprint
        staleWorkspacesRemoved = $script:Workspace.StaleWorkspacesRemoved
    }
    if ([string]::IsNullOrWhiteSpace($SessionCookie)) {
        $SessionCookie = [Environment]::GetEnvironmentVariable("PICTOFLUX_SESSION_COOKIE")
    }
    if ([string]::IsNullOrWhiteSpace($TurnstileToken)) {
        $TurnstileToken = [Environment]::GetEnvironmentVariable("PICTOFLUX_TURNSTILE_TOKEN")
    }
    $hasSession = -not [string]::IsNullOrWhiteSpace($SessionCookie)

    $script:Cancellation = [Threading.CancellationTokenSource]::new()
    $script:Cancellation.CancelAfter([TimeSpan]::FromSeconds($TimeoutSeconds))
    $script:HttpClient = New-PictoFluxClient -StableVisitorId $stableVisitorId -Cookie $SessionCookie

    Write-JobProgress "Checking selected model access" "preparing" "model_catalog" 0.08
    $modelRequest = @{
        Client = $script:HttpClient
        HasSession = $hasSession
        CancellationToken = $script:Cancellation.Token
    }
    $model = Get-SelectedModel @modelRequest
    $creditsByFeature = Get-PropertyValue $model "creditsByFeature"
    $featureCredits = Get-PropertyValue $creditsByFeature $Feature
    $baseCredits = if ($null -ne $featureCredits) {
        [double] $featureCredits
    } else {
        [double] $model.credits
    }
    $creditEstimate = @{
        BaseCredits = $baseCredits
        OutputImageCount = 1
        SelectedResolution = $Resolution
    }
    $requiredCredits = Get-EstimatedCredits @creditEstimate
    $billingSummary = $null
    $creditsBefore = $null
    $activePlanId = $null
    if ($hasSession) {
        $billingRequest = @{
            Client = $script:HttpClient
            CancellationToken = $script:Cancellation.Token
        }
        $billingSummary = Get-BillingSummary @billingRequest
        $creditsBefore = [double] $billingSummary.credits
        $activePlan = Get-PropertyValue $billingSummary "activePlan"
        $activePlanId = [string] (Get-PropertyValue $activePlan "id")
    }
    $normalizedPlanId = ([string] $activePlanId).ToLowerInvariant()
    $hasPaidPlan = $normalizedPlanId -in @("starter", "pro", "ultra")
    $turnstileRequired = -not $hasSession -or $requiredCredits -le 0 -or -not $hasPaidPlan
    $modelProgress = @{
        Text = "$($model.name) catalog entry resolved"
        Stage = "preparing"
        Phase = "model_resolved"
        Ratio = 0.16
        Details = @{
            requiredCredits = $requiredCredits
            baseCredits = $baseCredits
            resolutionMultiplier = [double] $ResolutionMultipliers[$Resolution]
            modelAvailable = [bool] $model.isAvailable
            turnstileRequired = $turnstileRequired
        }
    }
    Write-JobProgress @modelProgress

    if ($model.isAvailable -ne $true -and -not $ContractOnly) {
        $message = if ($hasSession) {
            "$($model.name) exists, but PictoFlux marks it unavailable for this session or plan."
        } else {
            "$($model.name) exists, but PictoFlux marks it unavailable without an authorized session or eligible plan."
        }
        Stop-Creation "creation.workspace_expired" $message
    }
    if ($model.supportedResolutions -notcontains $Resolution -and -not $ContractOnly) {
        Stop-Creation "creation.input_invalid" "The selected PictoFlux plan does not support the requested resolution."
    }
    if ($requiredCredits -gt 0 -and -not $hasSession -and -not $ContractOnly) {
        Stop-Creation "creation.workspace_expired" "Paid-credit generation requires an authenticated PictoFlux session."
    }
    if ($null -ne $creditsBefore -and $creditsBefore -lt $requiredCredits -and -not $ContractOnly) {
        Stop-Creation "creation.capacity_unavailable" "The PictoFlux account does not have enough credits for this request."
    }
    if ($turnstileRequired -and [string]::IsNullOrWhiteSpace($TurnstileToken) -and -not $ContractOnly) {
        Stop-Creation "creation.challenge_required" "PictoFlux requires a fresh Turnstile token for this free-tier request; the automation does not bypass challenges."
    }

    $maxInputsByFeature = Get-PropertyValue $model "maxInputImagesByFeature"
    $featureInputLimit = Get-PropertyValue $maxInputsByFeature $Feature
    if ($null -eq $featureInputLimit) {
        $featureInputLimit = [int] $model.maxInputImages
    }
    if ($sourceRecords.Count -gt [int] $featureInputLimit) {
        Stop-Creation "creation.input_invalid" "$($model.name) accepts at most $featureInputLimit input images for this operation."
    }

    Write-JobProgress "Preparing reference images" "preparing" "reference_image" 0.24
    $preparedReferences = @(
        foreach ($sourceRecord in $sourceRecords) {
            Prepare-ReferenceImage -SourceBytes $sourceRecord.Bytes -SourceInfo $sourceRecord.Info
        }
    )
    $preparedAggregateBytes = 0L
    foreach ($reference in $preparedReferences) {
        $preparedAggregateBytes += [long] $reference.Bytes.Length
    }
    if ($preparedAggregateBytes -gt $MaxPreparedAggregateBytes) {
        Stop-Creation "creation.input_invalid" "Prepared reference images exceed PictoFlux's 12 MB aggregate limit."
    }
    $resolvedAspectRatio = if ($preparedReferences.Count -gt 0) {
        $aspectRequest = @{
            Requested = $AspectRatio
            Width = $preparedReferences[0].Width
            Height = $preparedReferences[0].Height
        }
        Resolve-AspectRatio @aspectRequest
    } elseif ($AspectRatio -eq "auto") {
        "1:1"
    } else {
        $AspectRatio
    }
    $generationRequest = @{
        Model = $model
        PreparedReferences = $preparedReferences
        ResolvedAspectRatio = $resolvedAspectRatio
        ChallengeToken = $TurnstileToken
    }
    $generationBody = New-GenerationBody @generationRequest
    $requestBytes = [Text.Encoding]::UTF8.GetByteCount(($generationBody | ConvertTo-Json -Depth 8 -Compress))
    Write-JobProgress "Image request ready" "preparing" "request_ready" 0.38

    if ($ContractOnly) {
        Write-Ndjson ([ordered]@{
            ok = $true
            id = $script:JobId
            result = [ordered]@{
                contractOnly = $true
                provider = "PictoFlux"
                workspaceId = $script:Workspace.Id
                identityFingerprint = $script:Workspace.IdentityFingerprint
                workspaceIsolation = "fresh_per_invocation"
                workspaceCleanup = "exact_owned_on_exit"
                operation = $Operation
                modelName = [string] $model.name
                modelSlug = [string] $model.publicSlug
                modelId = [string] $model.id
                modelAvailable = [bool] $model.isAvailable
                baseCredits = $baseCredits
                requiredCredits = $requiredCredits
                resolutionMultiplier = [double] $ResolutionMultipliers[$Resolution]
                outputImageCount = 1
                inputReferenceCount = $preparedReferences.Count
                hasAuthenticatedSession = $hasSession
                activePlanId = $activePlanId
                creditsBefore = $creditsBefore
                turnstileRequired = $turnstileRequired
                resolution = $Resolution
                aspectRatio = $resolvedAspectRatio
                sourceImages = @(
                    foreach ($record in $sourceRecords) {
                        [ordered]@{
                            width = $record.Info.Width
                            height = $record.Info.Height
                            bytes = $record.Bytes.Length
                            mimeType = $record.Info.MimeType
                        }
                    }
                )
                preparedImages = @(
                    foreach ($reference in $preparedReferences) {
                        [ordered]@{
                            width = $reference.Width
                            height = $reference.Height
                            bytes = $reference.Bytes.Length
                            mimeType = $reference.MimeType
                        }
                    }
                )
                preparedAggregateBytes = $preparedAggregateBytes
                requestBytes = $requestBytes
                wouldSubmit = [bool] (
                    $model.isAvailable -eq $true -and
                    $model.supportedResolutions -contains $Resolution
                )
            }
        })
        exit 0
    }

    $generationCall = @{
        Client = $script:HttpClient
        Body = $generationBody
        CancellationToken = $script:Cancellation.Token
    }
    $imageReference = Invoke-Generation @generationCall
    Write-JobProgress "Downloading generated image" "finalizing" "artifact_download" 0.87
    $artifactDownload = @{
        Client = $script:HttpClient
        Reference = $imageReference
        CancellationToken = $script:Cancellation.Token
    }
    [byte[]] $generatedBytes = Get-GeneratedBytes @artifactDownload
    $generatedInfo = Get-ImageInfo $generatedBytes
    $generatedLimitCheck = @{
        Info = $generatedInfo
        Length = $generatedBytes.Length
        MaximumBytes = $MaxGeneratedBytes
        MaximumSide = $MaxGeneratedSide
        MaximumPixels = $MaxGeneratedPixels
    }
    Confirm-ImageLimits @generatedLimitCheck
    $artifact = Normalize-GeneratedImage -Bytes $generatedBytes -Info $generatedInfo
    Write-JobProgress "Verifying generated image" "finalizing" "artifact_verification" 0.94
    $published = Publish-Artifact -Directory $OutputDir -Artifact $artifact
    $creditsAfter = $null
    if ($hasSession) {
        try {
            $postBillingRequest = @{
                Client = $script:HttpClient
                CancellationToken = $script:Cancellation.Token
            }
            $postBilling = Get-BillingSummary @postBillingRequest
            $creditsAfter = [double] $postBilling.credits
        } catch {
            $creditsAfter = $null
        }
    }

    Write-Ndjson ([ordered]@{
        ok = $true
        id = $script:JobId
        result = [ordered]@{
            outputPath = $published.Path
            sizeBytes = $published.SizeBytes
            sha256 = $published.Sha256
            width = $artifact.Width
            height = $artifact.Height
            mimeType = $artifact.MimeType
            workspaceId = $script:Workspace.Id
            identityFingerprint = $script:Workspace.IdentityFingerprint
            workspaceIsolation = "fresh_per_invocation"
            workspaceCleanup = "exact_owned_on_exit"
            modelName = [string] $model.name
            modelSlug = [string] $model.publicSlug
            modelId = [string] $model.id
            requiredCredits = $requiredCredits
            baseCredits = $baseCredits
            resolutionMultiplier = [double] $ResolutionMultipliers[$Resolution]
            outputImageCount = 1
            inputReferenceCount = $preparedReferences.Count
            operation = $Operation
            creditsBefore = $creditsBefore
            creditsAfter = $creditsAfter
            resolution = $Resolution
            aspectRatio = $resolvedAspectRatio
        }
    })
    exit 0
} catch [OperationCanceledException] {
    $failure = [ordered]@{
        ok = $false
        id = $script:JobId
        errorCode = "creation.timed_out"
        error = "The PictoFlux image job timed out or was cancelled."
    }
    if ($null -ne $script:Workspace) {
        $failure.workspaceId = $script:Workspace.Id
        $failure.identityFingerprint = $script:Workspace.IdentityFingerprint
        $failure.workspaceCleanup = "exact_owned_on_exit"
    }
    Write-Ndjson $failure
    exit 1
} catch {
    $errorCode = "creation.failed"
    if ($_.Exception.Data.Contains("errorCode")) {
        $errorCode = [string] $_.Exception.Data["errorCode"]
    }
    $failure = [ordered]@{
        ok = $false
        id = $script:JobId
        errorCode = $errorCode
        error = $_.Exception.Message
    }
    if ($null -ne $script:Workspace) {
        $failure.workspaceId = $script:Workspace.Id
        $failure.identityFingerprint = $script:Workspace.IdentityFingerprint
        $failure.workspaceCleanup = "exact_owned_on_exit"
    }
    Write-Ndjson $failure
    exit 1
} finally {
    if ($null -ne $script:HttpClient) {
        $script:HttpClient.Dispose()
    }
    if ($null -ne $script:Cancellation) {
        $script:Cancellation.Dispose()
    }
    if ($null -ne $script:Workspace) {
        $workspaceCleanup = @{
            Root = $script:Workspace.Root
            WorkspacePath = $script:Workspace.Path
            ExpectedJobId = $script:Workspace.JobId
            ExpectedWorkspaceId = $script:Workspace.Id
            ExpectedIdentityFingerprint = $script:Workspace.IdentityFingerprint
        }
        Remove-OwnedJobWorkspace @workspaceCleanup | Out-Null
        $script:Workspace = $null
    }
}
