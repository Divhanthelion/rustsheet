# Microsoft Store release

RustSheet ships to the Store as an MSIX package. The Store re-signs the package, so no code-signing certificate is needed.

## One-time setup (Partner Center)

1. **Developer account.** Sign up at https://storedeveloper.microsoft.com as an Individual. Registration for individuals is free and needs ID verification.
2. **Reserve the name.** Partner Center > Apps and games > New product > **MSIX or PWA app** > reserve `RustSheet`. If it is taken, reserve a variant (e.g. "RustSheet Spreadsheet") and change `DisplayName` in [AppxManifest.xml](AppxManifest.xml) to match.
3. **Copy the identity.** Product > Product management > **Product identity**. Note these three values:
   - `Package/Identity/Name` (e.g. `12345Divhanthelion.RustSheet`)
   - `Package/Identity/Publisher` (`CN=...`)
   - `Package/Properties/PublisherDisplayName`
4. **CI variables (optional).** To have the Release workflow build Store-ready packages, add these as repository variables (Settings > Secrets and variables > Actions > Variables): `MSIX_IDENTITY_NAME`, `MSIX_PUBLISHER` and `MSIX_PUBLISHER_DISPLAY_NAME`. They are public identifiers, not secrets.

## Build the package

```powershell
.\packaging\windows\build-msix.ps1 `
    -IdentityName "<Package/Identity/Name>" `
    -Publisher "<Package/Identity/Publisher>" `
    -PublisherDisplayName "<PublisherDisplayName>"
```

The output is `target\msix\RustSheet_<version>_x64.msix`. The package version comes from `Cargo.toml` with a fourth field of `.0`, as the Store requires. **Bump `version` in Cargo.toml for every new submission**, because the Store rejects a version it has already seen.

Alternatively, push a tag `vX.Y.Z` and download `RustSheet_<version>.msixbundle` (x64 and Arm64 in one upload) from the GitHub release the workflow creates. Upload the bundle to the Store.

### Test before submitting

- **Install locally** (needs Developer Mode): `Add-AppxPackage -Register target\msix\stage\AppxManifest.xml`, launch RustSheet from Start, right-click a `.csv` > Open with > RustSheet. Remove it with `Get-AppxPackage *RustSheet* | Remove-AppxPackage`.
- **Certification kit**: `appcert.exe test -appxpackagepath target\msix\RustSheet_<version>_x64.msix -reportoutputpath wack.xml` (from `Windows Kits\10\App Certification Kit`, needs elevation). It passes with no flagged tests. Keep the `vendor/webbrowser` stand-in: the real crate made the optional "Blocked executables" test fail.

## Submission answers

| Section | Answer |
|---------|--------|
| Pricing | Free (or set a price; the app has no in-app purchases) |
| Markets | All |
| Category | Productivity |
| Subcategory | (none) |
| Privacy policy URL | https://github.com/Divhanthelion/rustsheet/blob/master/PRIVACY.md |
| Website | https://github.com/Divhanthelion/rustsheet |
| Support contact | https://github.com/Divhanthelion/rustsheet/issues |
| Age ratings (IARC) | No violence, sexual content, gambling, user interaction, location sharing or purchases. Expected rating: 3+ / Everyone |
| Restricted capability `runFullTrust` | "RustSheet is a native Win32 desktop application packaged as MSIX. It needs full trust to run as a desktop process and to open and save files the user chooses." |
| Product declarations | Does not access, collect or transmit personal information. |
| System requirements | Windows 10 version 1809 or later, x64. OpenGL 2.0+ graphics (any GPU driver from the last decade). |

## Store listing (en-us)

**Product name:** RustSheet

**Short description** (shown in search results):

> A fast, native spreadsheet with Excel-compatible formulas. Opens and saves .xlsx and .csv files, with no account and no cloud.

**Description:**

> RustSheet is a lightweight spreadsheet for Windows that opens instantly and keeps your data on your PC.
>
> Write formulas the way you already know: more than 220 Excel-compatible functions, including SUM, AVERAGE, IF, VLOOKUP, INDEX/MATCH, SUMIF and COUNTIF with wildcards, TEXT, TEXTJOIN, ROUND, date functions like EDATE and NETWORKDAYS, and financial functions like PMT, NPV and IRR. Results update as you type, and RustSheet catches circular references before they cause trouble.
>
> Work across sheets with references like Sheet2!A1. Rename a sheet and every formula that uses it updates. Turn a range into a line, bar, scatter, area, pie or doughnut chart in a couple of clicks.
>
> Make it look the way you want with bold and italic text, font colors and sizes, fills, borders, alignment, and number formats for currency, percentages, dates and times. Type 12%, $1,234.50 or 2026-10-03 and RustSheet formats it for you.
>
> Sort and filter your data, find and replace across sheets, and drag to fill a series of numbers, dates or months. Insert, delete, hide and freeze rows and columns, merge cells and wrap text. Print, or export a PDF to share.
>
> Summarize a list with a PivotTable: totals by region, by month, by anything, with filters, subtotals and grand totals, refreshed in one click when the data changes. Spot what matters with conditional formatting, from highlight rules to data bars and color scales. Keep entries tidy with drop-down lists and data validation, leave notes on cells, and drop in pictures or screenshots.
>
> Open the .xlsx files you already have, edit them, and save them back with their formulas, formatting, charts, notes, validation and pictures intact. PivotTables made in Excel open ready to refresh. Import and export CSV as well. If RustSheet ever closes unexpectedly, your unsaved work is waiting when you open it again.
>
> RustSheet has no account, no subscription, no ads and no telemetry. It never connects to the internet.
>
> It is open source under the MIT license.

**What's new in this version:**

> First release.

**Product features** (one per line, up to 20):

- 220+ Excel-compatible functions with autocomplete and built-in help
- Opens and saves Excel .xlsx files with formulas, formatting, charts and pictures
- PivotTables with filters, subtotals and one-click refresh
- Conditional formatting: highlight rules, data bars and color scales
- Data validation with drop-down lists
- Cell notes and pictures
- Fonts, colors, borders and number formats; typed dates and amounts format themselves
- Copy and paste with Excel and other apps
- Sort, filter, and find and replace
- Insert, delete, hide, resize and freeze rows and columns
- Fill series by dragging (numbers, dates, months, weekdays)
- Merged cells, wrapped text and full-size sheets
- Line, bar, scatter, area, pie and doughnut charts
- Multiple sheets with cross-sheet references
- Print, or export to PDF
- Autosave and crash recovery
- CSV import and export
- Instant recalculation, undo and redo
- Light and dark themes and Excel-style shortcuts
- No account, no ads, no telemetry; works fully offline

**Search terms** (up to 7):

`spreadsheet`, `xlsx`, `excel alternative`, `pivot table`, `formulas`, `charts`, `csv`

**Screenshots** (1500x1000; the Store accepts 1 to 10 at 1366x768 or larger):

1. [assets/screenshot.png](../../assets/screenshot.png): a formatted budget with a merged title, data bars, a color scale, filter buttons and a chart
2. [assets/screenshot-dark.png](../../assets/screenshot-dark.png): the same in dark mode
3. [assets/screenshot-filter.png](../../assets/screenshot-filter.png): a sales list filtered to two regions, with highlights
4. [assets/screenshot-pivot.png](../../assets/screenshot-pivot.png): a PivotTable of revenue by rep and region

Regenerate them after building (`cargo build --release`):

```powershell
cargo run --release --example demo_workbook -- target\demo.xlsx target\sales.xlsx target\pivot.xlsx
.\packaging\windows\screenshot.ps1
.\packaging\windows\screenshot.ps1 -Theme Dark -Out assets\screenshot-dark.png
.\packaging\windows\screenshot.ps1 -Workbook target\sales.xlsx -Out assets\screenshot-filter.png
.\packaging\windows\screenshot.ps1 -Workbook target\pivot.xlsx -Out assets\screenshot-pivot.png
```

**Store logos:** use [assets/icon-1024.png](../../assets/icon-1024.png) for the 1:1 box art and app tile icon (the Store asks for 300x300 or larger; it downscales).

**Copyright:** © 2026 Divhanthelion
