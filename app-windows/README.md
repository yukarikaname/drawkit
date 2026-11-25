# DrawKit for Windows (WinUI 3)

The Windows shell is a native WinUI 3 window around the same Rust core. Generic chrome uses the
Fluent controls as shipped: buttons, sliders, color picker, list, file pickers. The canvas is a
`WriteableBitmap`; the Rust GPU compositor keeps the layer textures resident, blends them on the
GPU and returns one image for the bitmap. A machine without an adapter (or without
`drawkit_gpu.dll`) falls back to the core's CPU composite. The zero-copy `SwapChainPanel` path is
wired through the panel's `ISwapChainPanelNative` interface.

## Build

This directory builds only on Windows, with the .NET 8 SDK and the Rust core as a DLL:

```powershell
rustup target add x86_64-pc-windows-msvc
cargo build --release --target x86_64-pc-windows-msvc -p drawkit-core -p drawkit-gpu
New-Item -ItemType Directory -Force app-windows/native | Out-Null
Copy-Item target/x86_64-pc-windows-msvc/release/drawkit_core.dll app-windows/native/drawkit_core.dll
Copy-Item target/x86_64-pc-windows-msvc/release/drawkit_gpu.dll app-windows/native/drawkit_gpu.dll
dotnet run --project app-windows/DrawKit.WinUI.csproj -p:Platform=x64
```

For ARM64 use `aarch64-pc-windows-msvc` and `-p:Platform=ARM64`.

The app is unpackaged (`WindowsPackageType=None`) and carries the Windows App SDK runtime
self-contained, so a folder build can be started directly. Packaging into MSIX comes with the
release pipeline.

## What is wired

- brush / eraser / fill with size, opacity, stabiliser and fill gap/grow/soft controls;
- mirror H / V;
- undo / redo;
- layer list with groups, fill layers, visibility and layer opacity;
- open `.dkp`, `.kra`, `.psd`, `.png`, `.jpg`, `.jpeg` at their own size; save `.dkp`;
- Windows Ink pressure through `PointerPoint.Properties.Pressure`;
- resident GPU layer compositing and zero-copy `SwapChainPanel` presentation, with the bitmap
  and CPU paths as fallbacks.

Not yet: HDR and the onboarding/settings resources from macOS.
