# Spike: create a true-size (1:1) printer setup automatically

Status: **Windows done** (ET-8500, network queue). macOS not started; a separate pass on the owner's Intel Mac.
Branch: `spike/printer-auto-setup`. All code under `spikes/printer-auto-setup/` is throwaway prototype code.

Evidence labels used throughout:

- 🧑 **measured or seen by the owner** on paper
- 🔎 **API check** on the owner's PC (driver or printer queried; no print)
- 📄 **documentation only** (sources linked)

## 1. Verdict on the hypothesis

> *"Custom paper sizes are a supported, reliable way to get around borderless expansion."*

**Refuted.**

- A custom size prints true size because **it isn't borderless**. The printer keeps its normal ~3 mm hardware margins, so it never needs to overspray, so it never enlarges anything.
- Nothing from Epson, Canon, HP, Microsoft or Apple describes custom sizes as a fix for expansion. 📄
- **Epson, Windows:** a User-Defined size can only be borderless at standard borderless widths. When it is borderless, it gets the same Auto Expand / Retain Size choice as any other size. On the ET-8500 the widths are 89.0, 101.6, 127.0, 203.2, 210.0 and 215.9 mm 🧑. See [Custom sizes on the ET-8500](#custom-sizes-on-the-et-8500-windows) 📄 ([ET-8500/8550 User's Guide p.295](https://files.support.epson.com/docid/cpd5/cpd59879/helpcontents.html), [Epson FAQ KA-01554](https://epson.co.uk/faq/KA-01554/contents?loc=en-us), [Epson Pro FAQ](https://epson.com/faq/SPT_C593001PRO~faq-68086))
- **macOS / CUPS:**
  - CUPS gives custom sizes the printer's *largest* hardware margins (`*HWMargins`), so the job reaches the printer as a bordered job. 📄 ([cups/ppd-cache.c](https://github.com/OpenPrinting/cups/blob/master/cups/ppd-cache.c))
  - An ET-8550 user reports that a zero-margin custom size "still prints with a border". 📄 ([Adobe community](https://community.adobe.com/questions-675/borderless-printing-missing-on-mac-os-15-2-980352))
  - The owner's Mac result is consistent with this. *To check:* do those Mac custom-size prints have white edges?
- **The official switches are vendor-specific:**
  - Epson: **Retain Size** on Windows; an "Expansion" Min/Mid/Max option on Mac (`EPIJ_exmg`, already parsed in `src-tauri/src/printing.rs`).
  - Canon: an "Amount of Extension" slider whose leftmost position means full size. 📄 ([Canon](https://ij.manual.canon/ij/webmanual/PrinterDriver/W/TS8700%20series/CO/EN/PPG/dg-c_borderless.html))
  - IPP: an optional `media-overprint` attribute ([PWG 5100.13-2023 §6.2.2](https://ftp.pwg.org/pub/pwg/candidates/cs-ippnodriver20-20230301-5100.13.pdf)). **The ET-8500 does not support it.** 🔎

## 2. Headline answer

**Windows, ET-8500: yes, technically.** Hello Print can create a true-size borderless setup **with no driver dialog and no saved blob**, and verify it before printing. **Decision:** don't ship the byte patch for now. Ship a guided, pre-filled driver dialog with the same verification instead (section 6). It works for every printer, at a fraction of the maintenance.

1. **Named settings.** Start from the queue's defaults and set size, borderless, paper source, paper type, quality, orientation and colour by name through a **Print Ticket**. The driver validates the choices and converts them into its own settings.
2. **Retain Size.** Set the **Retain Size flag**, a single byte in Epson's private settings. This is guarded by driver version and blob size.
3. **Verify.** Read the result back. Check that every setting stuck, and that the **logical dpi equals the print-quality dpi**, which means the driver isn't expanding.

The result is identical to the owner's hand-made Epson presets for 4x6 and Letter, apart from one unused field. 🔎 Printed and confirmed by the owner, with and without Retain Size. 🧑

**Windows, other printers:** the named settings are generic, but expansion is not, because each vendor stores it privately. The same guided dialog, dpi check and 50 mm test print apply. First external test: an HP DeskJet 3632.

**macOS:** not tested yet. The Epson driver's `EPIJ_exmg` option and CUPS `cupsBorderlessScalingFactor` (both already read by Hello Print) are the starting points.

**A production bug found on the way:** Hello Print prints Retain Size jobs **off-centre**, shifted ~2.7 mm left and ~1.4 mm up. Filed as [#26](https://github.com/jakeklassen/hello-print/issues/26), with a prototype fix verified on paper. 🧑

## 3. How the ET-8500 driver does it (Windows)

### Settings exposed by name (Print Ticket) 🔎

| Epson dialog | Print Ticket feature | Value for the magnet presets |
|---|---|---|
| Paper Source | `psk:PageInputBin` | `ns0000:AutoSheetFeeder` ("Rear Paper Feeder") |
| Document Size | `psk:PageMediaSize` (incl. `psk:CustomMediaSize` "User-Defined") | `ns0000:Fullsize4x6`, `psk:NorthAmericaLetter` |
| Borderless | `psk:PageBorderless` | `psk:Borderless` |
| Paper Type | `psk:PageMediaType` | `psk:Bond` ("Presentation Paper Matte") |
| Quality | `psk:PageOutputQuality` (+ `psk:PageResolution` detail) | `ns0000:HighQuality` |
| Orientation, Color, Copies, Collate, Multi-Page | standard `psk:` features | Portrait, Color |
| **Borderless → Settings… (Auto Expand / Retain Size, amount)** | **not exposed** | stored only in the private bytes |
| Reverse Order, Quiet Mode, Print Preview, Job Arranger Lite | not exposed | irrelevant to 1:1 |

Settings the driver doesn't model travel inside the ticket as `ns0000:PageDevmodeSnapshot`, a base64 copy of the whole DEVMODE. 📄 ([Microsoft](https://learn.microsoft.com/en-us/archive/blogs/benkuhn/adding-printticket-support-to-a-monolithic-driver))

### Retain Size is one byte 🔎🧑

Two captures from the driver dialog, identical except for the expansion method, differ in exactly one meaningful byte.

| | value | logical dpi | page reported to the app |
|---|---|---|---|
| Retain Size | DEVMODE byte **7942 = `01`** | **720** (= print quality) | 106.96 × 155.22 mm (bigger than the 101.6 × 152.4 sheet) |
| Auto Expand | DEVMODE byte **7942 = `00`** | **758** | 101.57 × 152.37 mm (= the sheet) |

What this means:

- **The offset is version-specific.** It applies to driver `dmDriverVersion 0x300`, `dmDriverExtra 7972`, with Windows driver package `EPSON ET-8500 Series`, MajorVersion 3.
- **The byte round-trips.** Setting it in code and validating with `DocumentProperties(DM_IN_BUFFER | DM_OUT_BUFFER)` produces the same blob and the same geometry as the dialog does.
- **A 2-byte value at offset 2304** changes when the dialog is used (`12 27` → `00 00`). It has no effect on geometry or output; the dialog doesn't restore it.

**How Auto Expand works:** the driver *misreports its resolution* (758 instead of 720 dpi). The app renders for 758 dpi and the printer lays it down at 720, so everything prints **×1.0528**: a 50 mm square becomes ~52.6 mm.

**How Retain Size works:** the driver reports a page *larger* than the sheet, at the true dpi. The extra is bleed room, and the paper sits centred within it.

**A free expansion check** (no printing): compare `GetDeviceCaps(LOGPIXELSX)` with the DEVMODE's `dmPrintQuality`. If they're equal, there's no expansion. If logical is higher, the print is expanding by logical ÷ quality. Proven on this driver; its generality to other vendors is untested.

### Why prints were off-centre ([#26](https://github.com/jakeklassen/hello-print/issues/26)) 🔎🧑

`watcher.rs` draws the image at `(0, 0)` with the paper's size. With Retain Size, `(0, 0)` is the corner of the larger page, not of the sheet. The fix is to draw at `((physical − paper) / 2 − physicalOffset)` on each axis, which works out to +2.68 / +1.41 mm here. That is a no-op whenever the page equals the sheet.

### Custom sizes on the ET-8500 (Windows) 🧑

Observed in the driver dialog (2026-10-19): **Document Size → User-Defined → Settings…**.

- **Borderless ticked:** Paper Width becomes a dropdown of **89.0, 101.6, 127.0, 203.2, 210.0 and 215.9 mm** (3.5 / 4 / 5 / 8 in, A4 width, 8.5 in). Paper Height is free from **86 to 2000 mm**. A borderless User-Defined size still gets the Auto Expand / Retain Size choice.
- **Borderless off:** Paper Width is a free number box and goes much lower, but prints keep the ~3 mm margins.

**Rule:** a custom size can be borderless if **either** edge is one of those widths (feed that edge first) **and** the other edge is 86–2000 mm. Width is the edge across the feed.

Applied to the custom sizes Makers use on the platform (as of 2026-10-19):

| Size (mm) | What it is | Templates | Borderless on the ET-8500? |
|---|---|---|---|
| 107.95 × 279.4 | 4.25 × 11 in, Letter cut lengthwise | 9 | ❌ neither edge is a borderless width, so it prints bordered. To go edge to edge, print two-up on Letter and cut. |
| 139.7 × 215.9 | Half Letter | 4 | ✅ feed the 215.9 edge first (width 215.9, height 139.7) |
| 107.95 × 91.44 | 4.25 × 3.6 in | 1 | ❌ bordered |
| 80.01 × 80.01 | 80 mm square | 1 | ❌ no borderless width, and 80 mm is under the 86 mm minimum height |
| 148.5 × 210 | half A4 (A5 is 148 × 210) | 1 | ✅ feed the 210 edge first (width 210, height 148.5) |
| 215.9 × 355.6 | Legal | 1 | ✅ as is |
| 279.4 × 215.9 | Letter, landscape | 1 | ✅ it's Letter: feed portrait and rotate the design |

**How Hello Print would support a custom size** (for when Makers ask; not built):

1. **Test sheet:** the generator (`src-tauri/src/test_sheet.rs`) already renders any width × height. Pages under 100 mm get a 25 mm centre square. Only a width/height input in Settings → Test Sheets is missing.
2. **Setup:** no new driver code. In the guided dialog the Maker creates a User-Defined (Borderless) size and picks Retain Size. The expansion check and test sheet verify it as for any other size. Optionally, pre-fill the size through Print Ticket `psk:CustomMediaSize`, which the driver exposes.
3. **Routing:** the filename has to carry the size (e.g. `4.25x11` or `108x279mm`), so the platform must emit it, and presets need to store custom dimensions. This is the "CustomKeyword (V2)" in CLAUDE.md.
4. **Orientation:** when the size is borderless only rotated (Half Letter, half A4), the image has to be rotated to match the feed edge. Hello Print would do that from the preset's dimensions.

Other printers have their own width lists. Their dialogs enforce them the same way, and the test sheet shows the result.

## 4. Test prints

All on 4x6 Presentation Paper Matte, rear feeder, network queue. Sheets come from `spikes/printer-auto-setup/test-sheets/make-4x6-sheets.ps1`: 300 dpi, whole-pixel dimensions, thin outlines.

| # | Setup | Print path | Result | Evidence |
|---|---|---|---|---|
| 1 | Owner's hand-made Epson preset (Retain Size): **baseline** | Hello Print 0.1.14, watch folder | Squares **50 × 50 mm** ✅. Image shifted left: white strip on the right, left edge clipped | 🧑 measured |
| 2 | Same blob | `print-centered.ps1` on the spike queue | Band and frame on **all four edges**, no white strip | 🧑 photo |
| 3 | **Built with no dialog**, Retain Size byte on | `print-centered.ps1` | Edges covered, as in #2 ("pretty clear that worked") | 🧑 seen |
| 4 | **Built with no dialog**, Retain Size byte off: **known bad** | `print-centered.ps1` | Blue edge band gone on all edges (image enlarged past the paper) | 🧑 seen |

Not recorded: the 140 mm line, and the exact size of the Auto Expand square (predicted ~52.6 mm).

## 5. Feasibility matrix

✅ works · ⚠️ partly, unverified or conditional · ❌ no. The emoji after each mark is the kind of evidence.

| Route | Custom size | Borderless | Expansion off | Tray | Paper type | Quality | Persists | Needs admin | Network / USB |
|---|---|---|---|---|---|---|---|---|---|
| **Win: named settings + Retain Size byte (ET-8500)** | ✅ borderless when one edge is 89 / 101.6 / 127 / 203.2 / 210 / 215.9 mm and the other is 86–2000 mm 🧑; otherwise bordered | ✅ 🧑 | ✅ 🧑 | ✅ 🔎 | ✅ 🔎 | ✅ 🔎 | ✅ the stored blob is re-verified each time 🔎 | ✅ no | ✅ network 🧑 / ⚠️ USB untested |
| Win: named settings, other printers | ⚠️ driver-dependent | ✅ 🔎 (standard feature) | ❌ needs the driver dialog | ✅ 🔎 | ✅ 🔎 | ✅ 🔎 | ✅ | ✅ no | ⚠️ untested |
| Win: shipped known-good blob per model | — | ✅ | ✅ | ✅ | ✅ | ✅ | ⚠️ the driver accepted it on a 2nd queue unchanged 🔎; other PCs/versions unknown | ✅ no | ⚠️ |
| Win: Windows forms (`AddForm`) | ⚠️ | ❌ 📄 | ❌ | — | — | — | machine-wide | ❌ admin 📄 | — |
| Win: import an Epson driver preset | — | — | — | — | — | — | — | — | ❌ no documented import/export 📄 |
| IPP direct to printer (both OSes) | ❌ no custom ranges 🔎 | ✅ zero-margin 4x6/Letter 🔎 | ❌ no `media-overprint` 🔎; firmware behaviour unprinted | ✅ `rear` 🔎 | ✅ 🔎 | ✅ 🔎 | n/a (per job) | ✅ no | network only |
| macOS: Epson driver (`EPIJ_exmg`) | — | ⚠️ 📄 | ⚠️ Min/Mid/Max only; no "retain" documented 📄 | ⚠️ | ⚠️ | ⚠️ | ⚠️ | ⚠️ | not tested |
| macOS: custom paper size | ✅ 📄 (`PMPaperCreateCustom`) | ❌ prints bordered 📄 | n/a | — | — | — | — | — | not tested |

IPP facts 🔎 (Get-Printer-Attributes over IPPS):
- **Formats:** JPEG, PWG-Raster, URF and ESC/P-R (no PDF).
- **Scaling:** `print-scaling` `none` is supported.
- **Margins:** 0 or 3 mm.
- **Missing:** `media-overprint`.

## 6. Recommendation (decided 2026-10-17)

**Guided setup for every printer, verified by the dpi check. No per-model byte patching for now.**

Patching Retain Size works on the ET-8500, as shown above, but it only saves the maker one dialog step, once per preset. The cost is a model-and-driver-version registry that needs a capture diff, a test printer and a release for every new model and every Epson driver update. Its fallback would be the guided dialog anyway, so the guided flow has to exist regardless. The byte finding stays documented here as a possible later upgrade, worth it only if support data shows makers routinely getting stuck.

**The Windows flow:**

1. The maker picks printer and paper size.
2. Hello Print opens the **driver dialog pre-filled** with the paper size, borderless and portrait. It uses only standard Print Ticket names, so it works on any Windows driver with no per-model knowledge.
3. The maker sets the expansion option, guided by a short screenshot or video per brand (Epson: Borderless → Settings… → Retain Size; Canon: Amount of Extension → leftmost).
4. On OK, Hello Print saves the setup and **immediately runs the dpi check**:
   - ✅ **True size:** logical dpi = print-quality dpi.
   - ⚠️ **"Enlarges prints by 5.3%":** logical dpi is higher. Reopen the dialog.
   - ❔ **"Can't tell on this printer":** the driver reports quality as a code (negative `dmPrintQuality`, as some HP drivers do) or doesn't change dpi when expanding.
5. An optional **50 mm test print** gives final confirmation, and is the only proof on a ❔ printer.
6. Prints are centred on the sheet ([#26](https://github.com/jakeklassen/hello-print/issues/26)).

**Later, not now:**
- A per-print re-check in the folder-watching pipeline.
- Pre-shrinking images on printers that can only expand (the `scale_compensation` idea).

**Templates:** with Retain Size, Epson warns of possible hairline white slivers from paper-feed tolerance. Designs that run to the edge should keep critical content more than ~0.5 mm inside it, and ideally include 1–2 mm of bleed.

## 7. What a preset stores

**Inputs:** printer (queue), paper size keyword.

Each preset keeps today's fields, including the captured `devmode_base64`, plus the result of the check from when the setup was saved:

```jsonc
{
  "printer_id": "ET-8500 Series(Network)",
  "paper_size_keyword": "4x6",
  "devmode_base64": "…",                       // captured from the pre-filled driver dialog
  "expansion_check": {
    "status": "true_size",                      // "true_size" | "expanding" | "unknown"
    "logical_dpi": 720, "print_quality_dpi": 720,
    "factor": 1.0,                              // logical ÷ quality when expanding (e.g. 1.0528)
    "page_mm": [106.96, 155.22], "paper_mm": [101.6, 152.4],
    "checked_at": "2026-10-17T12:00:00Z"
  }
}
```

No model registry: nothing is keyed by driver name or version.

## 8. Open risks

1. **ET-8550 untested.** It probably shares the driver family, but the byte layout is unknown until captured.
2. **Other PCs, locales and driver versions.** Print Ticket option *names* don't depend on language, but this is untested beyond one PC.
3. **USB queue untested.** Only the EpsonNet network queue was used.
4. **The dpi check outside Epson.** It's proven on this driver only.
5. **Hairline slivers** with Retain Size near the paper edge. Epson documents this; the owner's prints didn't show it.
6. **The 2-byte field at offset 2304** is unexplained. It is harmless in every test.
7. **macOS not done.**

## 9. Prototype code

| Path | What it does |
|---|---|
| `spikes/printer-auto-setup/windows/probe-capabilities.ps1` | Dumps PrintCapabilities, converts DEVMODE to Print Ticket, reports page geometry (read-only) |
| `spikes/printer-auto-setup/windows/capture-devmode.ps1` | Validates a saved DEVMODE on a queue, or captures one from the driver dialog |
| `spikes/printer-auto-setup/windows/diff-devmode.mjs` | Diffs two DEVMODEs (public fields and private bytes) |
| `spikes/printer-auto-setup/windows/build-profile.ps1` | **Builds a 1:1 setup with no dialog**, then verifies it |
| `spikes/printer-auto-setup/windows/print-centered.ps1` | Hello Print's print path with the centring fix; `-DryRun` shows the offsets |
| `spikes/printer-auto-setup/ipp/get-attributes.mjs` | IPP Get-Printer-Attributes (falls back to IPPS) |
| `spikes/printer-auto-setup/test-sheets/make-4x6-sheets.ps1` | The 4x6 measurement sheets |

Captured blobs and dumps stay in `tmp/spike/` (gitignored). They come from the owner's printer and aren't committed.

## 10. Spike housekeeping

- A test queue, **"HelloPrint Spike ET-8500"**, was added for the same printer (same driver and port). It was removed on 2026-10-17, and the default printer was restored to `ET-8500 Series(Network)`.
- The owner's queue `ET-8500 Series(Network)`, its defaults and the Epson presets were only ever read. The default printer is unchanged.
