# Extended range

DrawKit stores every channel as `f32` in the document's working space. An HDR document lets a
brush write above paper white; the flag only tells the shell how to present and export it. Nothing
in the display path tone maps: if a shell has no extended drawable, the encoded eight-bit composite
clips at paper white exactly like an SDR viewer would.

## Display

| Shell | Path |
|---|---|
| macOS | `CAMetalLayer`, `Rgba16Float`, extended Display P3 |
| iPad | same surface and colour space as macOS |
| Windows | WinUI `SwapChainPanel` through wgpu; scRGB when DXGI exposes it, PQ when the compositor only takes BT.2100 |
| Android | `Bitmap.Config.RGBA_F16` in linear extended sRGB, half floats written by JNI, HDR window colour mode on API 34+ |
| Linux | `GdkMemoryTexture` in `R16g16b16a16FloatPremultiplied` with the BT.2100 PQ colour state |

The shared wgpu surface asks the window system which colour spaces it can take, in this order:
extended Display P3, extended linear sRGB (scRGB), then BT.2100 PQ. The paper shader converts the
core's linear Display P3 composite into that signal. The eight-bit path is used only when no float
swapchain exists.

## Storage and export

- A painted HDR layer is not stored in the 16-bit PNG cache: that format clamps at one. Its tape
  events replay on open instead. Imported or baked layers keep their raster.
- The canvas exports to `f32` OpenEXR in linear Display P3, under the P3 chromaticities.
- HDR layer export writes one EXR per layer instead of a PNG.
- The `.dkp` manifest carries the dynamic range flag, so reopening a project restores the preview.

## Still open

- 16-bit PNG import currently goes through an eight-bit decode; a 16-bit source loses precision.
- PQ export to video (HDR10/HLG) is not implemented; EXR is the interchange for now.
- A Linux compositor that accepts the PQ texture is required for the Linux path to reach an HDR
  display; GTK will otherwise map the same colour state to SDR.
