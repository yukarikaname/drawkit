using System;
using System.IO;
using System.Collections.Generic;
using System.Linq;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Controls.Primitives;
using Microsoft.UI.Input;
using Microsoft.UI.Xaml.Input;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Media.Imaging;
using Windows.UI.Core;
using Windows.Foundation;
using Windows.Graphics;
using Windows.Storage.Pickers;
using Windows.System;
using Windows.UI;

namespace DrawKit;

public sealed partial class MainWindow : Window
{
    private PaintEngine? _engine;
    private string _tool = "brush";
    private float[] _color = { 0.10f, 0.09f, 0.08f, 1f };
    private bool _drawing;
    private double _lastX;
    private double _lastY;
    private double _zoom = 1;
    private double _panX;
    private double _panY;
    private bool _panning;
    private Point _panStart;
    private string _layerQuery = "";

    public MainWindow()
    {
        InitializeComponent();
        Closed += (_, _) =>
        {
            CancelFillAnimation();
            _engine?.Dispose();
        };
    }

    private void OnLoaded(object sender, RoutedEventArgs e)
    {
        _engine = new PaintEngine(1024, 1024);
        CanvasImage.Source = _engine.Bitmap;
        AttachSurface();
        ColorPicker.Color = Color.FromArgb(255, 24, 23, 21);
        ApplyColor();
        MirrorToggle.IsOn = false;
        MirrorBothToggle.IsOn = false;
        MirrorAngleSlider.Value = 90;
        HdrToggle.IsOn = _engine.IsHdr;
        ExposureSlider.Value = _engine.Exposure;
        RefreshLayers();
        Root.Focus(FocusState.Programmatic);
        UpdateStatus();
    }

    private void ApplyColor()
    {
        if (ColorPicker is null || OpacitySlider is null) return;
        var color = ColorPicker.Color;
        var gain = (float)Math.Pow(2, GlowSlider?.Value ?? 0);
        _color = new[]
        {
            color.R / 255f * gain,
            color.G / 255f * gain,
            color.B / 255f * gain,
            color.A / 255f * (float)OpacitySlider.Value,
        };
        UpdateStatus();
    }

    private void ColorChanged(ColorPicker sender, ColorChangedEventArgs args) => ApplyColor();
    private void OpacityChanged(object sender, RangeBaseValueChangedEventArgs e) => ApplyColor();

    private void SteadyChanged(object sender, RangeBaseValueChangedEventArgs e)
    {
        _engine?.SetStabilization((float)e.NewValue);
    }

    private void ToolClicked(object sender, RoutedEventArgs e)
    {
        if (sender is not ToggleButton button || button.Tag is not string tool) return;
        _tool = tool;
        SyncToolButtons();
        UpdateStatus();
    }

    private void AutoShapeToggled(object sender, RoutedEventArgs e) =>
        _engine?.SetAutoShape(AutoShapeToggle.IsOn);

    private void MirrorToggled(object sender, RoutedEventArgs e) => ApplyMirror();

    private void MirrorAngleChanged(object sender, RangeBaseValueChangedEventArgs e) => ApplyMirror();

    private void ApplyMirror()
    {
        if (_engine is null || MirrorToggle is null || MirrorAngleSlider is null || MirrorBothToggle is null)
            return;
        if (MirrorToggle.IsOn)
            _engine.SetMirrorAngle((float)MirrorAngleSlider.Value, MirrorBothToggle.IsOn);
        else
            _engine.SetMirror(0);
    }

    private void SyncToolButtons()
    {
        BrushButton.IsChecked = _tool == "brush";
        EraserButton.IsChecked = _tool == "eraser";
        FillButton.IsChecked = _tool == "fill";
        EyedropperButton.IsChecked = _tool == "eyedropper";
        RailBrushButton.IsChecked = _tool == "brush";
        RailEraserButton.IsChecked = _tool == "eraser";
        RailFillButton.IsChecked = _tool == "fill";
        RailEyedropperButton.IsChecked = _tool == "eyedropper";
    }

    private void UndoClicked(object sender, RoutedEventArgs e)
    {
        FinishFillAnimation();
        if (_engine?.Undo() == true) RefreshLayers();
    }

    private void RedoClicked(object sender, RoutedEventArgs e)
    {
        FinishFillAnimation();
        if (_engine?.Redo() == true) RefreshLayers();
    }

    private void CanvasPressed(object sender, PointerRoutedEventArgs e)
    {
        if (_engine is null) return;
        Root.Focus(FocusState.Programmatic);
        var current = e.GetCurrentPoint(CanvasHost);
        if (current.Properties.IsMiddleButtonPressed || current.Properties.IsRightButtonPressed)
        {
            _panning = true;
            _panStart = current.Position;
            CanvasHost.CapturePointer(e.Pointer);
            return;
        }
        var point = current;
        var document = ToDocument(point.Position);
        if (_tool == "eyedropper")
        {
            if (_engine.Pick((int)document.X, (int)document.Y) is { } picked)
            {
                ColorPicker.Color = Color.FromArgb(
                    (byte)Math.Clamp(picked[3] * 255, 0, 255),
                    (byte)Math.Clamp(picked[0] * 255, 0, 255),
                    (byte)Math.Clamp(picked[1] * 255, 0, 255),
                    (byte)Math.Clamp(picked[2] * 255, 0, 255));
                ApplyColor();
            }
            return;
        }
        if (_tool == "fill")
        {
            BeginFill((int)document.X, (int)document.Y);
            return;
        }
        FinishFillAnimation();
        _drawing = true;
        CanvasHost.CapturePointer(e.Pointer);
        _engine.Begin();
        var start = PenDynamics(point);
        _engine.Segment(document.X, document.Y, document.X, document.Y,
            (float)SizeSlider.Value / 2f, Pressure(point),
            start.tiltX, start.tiltY, start.azimuth, start.roll,
            _color, _tool == "eraser");
        _lastX = document.X;
        _lastY = document.Y;
        UpdateStatus();
    }

    private void CanvasMoved(object sender, PointerRoutedEventArgs e)
    {
        if (_engine is null) return;
        var point = e.GetCurrentPoint(CanvasHost);
        if (_panning)
        {
            _panX += point.Position.X - _panStart.X;
            _panY += point.Position.Y - _panStart.Y;
            _panStart = point.Position;
            ApplyTransform();
            return;
        }
        if (!_drawing) return;
        var document = ToDocument(point.Position);
        var dynamics = PenDynamics(point);
        _engine.Segment(_lastX, _lastY, document.X, document.Y,
            (float)SizeSlider.Value / 2f, Pressure(point),
            dynamics.tiltX, dynamics.tiltY, dynamics.azimuth, dynamics.roll,
            _color, _tool == "eraser");
        _lastX = document.X;
        _lastY = document.Y;
    }

    private void CanvasReleased(object sender, PointerRoutedEventArgs e)
    {
        if (_panning)
        {
            _panning = false;
            CanvasHost.ReleasePointerCapture(e.Pointer);
            return;
        }
        if (!_drawing) return;
        _drawing = false;
        CanvasHost.ReleasePointerCapture(e.Pointer);
        _engine?.End();
        _engine?.AutoShape();
        RefreshLayers();
    }

    private void CanvasWheel(object sender, PointerRoutedEventArgs e)
    {
        var point = e.GetCurrentPoint(CanvasHost);
        var factor = point.Properties.MouseWheelDelta > 0 ? 1.1 : 1 / 1.1;
        ZoomAt(point.Position, factor);
        e.Handled = true;
    }

    private void CanvasSizeChanged(object sender, SizeChangedEventArgs e)
    {
        ApplyTransform();
        ResizeSurface();
    }

    private double RasterScale => CanvasHost.XamlRoot?.RasterizationScale ?? 1;

    private void AttachSurface()
    {
        if (_engine is null) return;
        var width = (uint)Math.Max(1, CanvasHost.ActualWidth * RasterScale);
        var height = (uint)Math.Max(1, CanvasHost.ActualHeight * RasterScale);
        if (_engine.AttachSwapChainPanel(CanvasSwapChain, width, height))
        {
            CanvasSwapChain.Visibility = Visibility.Visible;
            CanvasImage.Visibility = Visibility.Collapsed;
        }
    }

    private void ResizeSurface()
    {
        if (_engine is null) return;
        if (!_engine.HasSurface)
        {
            AttachSurface();
            return;
        }
        var width = (uint)Math.Max(1, CanvasHost.ActualWidth * RasterScale);
        var height = (uint)Math.Max(1, CanvasHost.ActualHeight * RasterScale);
        _engine.ResizeSurface(width, height);
    }

    private void FitClicked(object sender, RoutedEventArgs e)
    {
        _zoom = 1;
        _panX = 0;
        _panY = 0;
        ApplyTransform();
        UpdateStatus();
    }

    private void ZoomAt(Point point, double factor)
    {
        var old = _zoom;
        _zoom = Math.Clamp(_zoom * factor, 0.25, 8);
        if (Math.Abs(_zoom - old) < 0.0001) return;
        var centerX = CanvasHost.ActualWidth / 2;
        var centerY = CanvasHost.ActualHeight / 2;
        var ratio = _zoom / old;
        _panX = point.X - centerX - (point.X - centerX - _panX) * ratio;
        _panY = point.Y - centerY - (point.Y - centerY - _panY) * ratio;
        ApplyTransform();
        UpdateStatus();
    }

    private void ApplyTransform()
    {
        CanvasHost.RenderTransform = new CompositeTransform
        {
            ScaleX = _zoom,
            ScaleY = _zoom,
            TranslateX = _panX,
            TranslateY = _panY,
            CenterX = CanvasHost.ActualWidth / 2,
            CenterY = CanvasHost.ActualHeight / 2,
        };
    }

    private Point ToDocument(Point point)
    {
        if (_engine is null) return point;
        var width = CanvasHost.ActualWidth;
        var height = CanvasHost.ActualHeight;
        var scale = Math.Min(width / _engine.Width, height / _engine.Height);
        if (scale <= 0) return point;
        var centerX = width / 2;
        var centerY = height / 2;
        var x = (point.X - centerX - _panX) / _zoom + centerX;
        var y = (point.Y - centerY - _panY) / _zoom + centerY;
        var originX = (width - _engine.Width * scale) / 2;
        var originY = (height - _engine.Height * scale) / 2;
        return new Point(
            Math.Clamp((x - originX) / scale, 0, _engine.Width - 1),
            Math.Clamp((y - originY) / scale, 0, _engine.Height - 1));
    }

    private static float Pressure(Microsoft.UI.Input.PointerPoint point)
    {
        var value = point.Properties.Pressure;
        return value > 0.01f ? Math.Clamp(value, 0.05f, 1f) : 1f;
    }

    private static (float tiltX, float tiltY, float azimuth, float roll) PenDynamics(
        Microsoft.UI.Input.PointerPoint point)
    {
        var properties = point.Properties;
        var tiltX = properties.XTilt;
        var tiltY = properties.YTilt;
        return (tiltX, tiltY, (float)Math.Atan2(tiltY, tiltX), properties.Twist);
    }

    private async void OpenClicked(object sender, RoutedEventArgs e)
    {
        if (_engine is null) return;
        var picker = new FileOpenPicker();
        foreach (var extension in new[] { ".dkp", ".kra", ".psd", ".png", ".jpg", ".jpeg" })
        {
            picker.FileTypeFilter.Add(extension);
        }
        WinRT.Interop.InitializeWithWindow.Initialize(
            picker, WinRT.Interop.WindowNative.GetWindowHandle(this));
        var file = await picker.PickSingleFileAsync();
        if (file is not null && _engine.Open(file.Path))
        {
            CanvasImage.Source = _engine.Bitmap;
            HdrToggle.IsOn = _engine.IsHdr;
            ExposureSlider.Value = _engine.Exposure;
            RefreshLayers();
            UpdateStatus();
        }
    }

    private async void SaveClicked(object sender, RoutedEventArgs e)
    {
        if (_engine is null) return;
        var picker = new FileSavePicker();
        picker.FileTypeChoices.Add("DrawKit project", new List<string> { ".dkp" });
        picker.FileTypeChoices.Add("OpenEXR", new List<string> { ".exr" });
        picker.SuggestedFileName = "painting";
        WinRT.Interop.InitializeWithWindow.Initialize(
            picker, WinRT.Interop.WindowNative.GetWindowHandle(this));
        var file = await picker.PickSaveFileAsync();
        if (file is null) return;
        var exr = Path.GetExtension(file.Path).Equals(".exr", StringComparison.OrdinalIgnoreCase);
        var saved = exr ? _engine.SaveExr(file.Path) : _engine.Save(file.Path);
        if (!saved)
        {
            Status.Text = "Could not save";
        }
    }

    private static bool ControlDown =>
        InputKeyboardSource.GetKeyStateForCurrentThread(VirtualKey.Control)
            .HasFlag(CoreVirtualKeyStates.Down);

    private void OnKeyDown(object sender, KeyRoutedEventArgs e)
    {
        if (ControlDown)
        {
            switch (e.Key)
            {
                case VirtualKey.Z:
                    UndoClicked(this, new RoutedEventArgs());
                    e.Handled = true;
                    break;
                case VirtualKey.Y:
                    RedoClicked(this, new RoutedEventArgs());
                    e.Handled = true;
                    break;
                case VirtualKey.S:
                    SaveClicked(this, new RoutedEventArgs());
                    e.Handled = true;
                    break;
                case VirtualKey.O:
                    OpenClicked(this, new RoutedEventArgs());
                    e.Handled = true;
                    break;
            }
            return;
        }
        switch (e.Key)
        {
            case VirtualKey.B:
                _tool = "brush";
                SyncToolButtons();
                break;
            case VirtualKey.E:
                _tool = "eraser";
                SyncToolButtons();
                break;
            case VirtualKey.F:
                _tool = "fill";
                SyncToolButtons();
                break;
            case VirtualKey.I:
                _tool = "eyedropper";
                SyncToolButtons();
                break;
            case VirtualKey.F2:
                RenameLayerClicked(this, new RoutedEventArgs());
                e.Handled = true;
                break;
        }
        UpdateStatus();
    }

    private void UpdateStatus()
    {
        if (_engine is null) return;
        var hdr = _engine.IsHdr ? (_engine.HdrSurface ? " · HDR" : " · HDR (8-bit display)") : "";
        Status.Text = $"{_engine.Width} × {_engine.Height} · {_tool} · {_zoom * 100:0}%{hdr}";
        CanvasHint.Visibility = _engine.IsBlank ? Visibility.Visible : Visibility.Collapsed;
        ColourWellFill.Background = new SolidColorBrush(ColorPicker.Color);
    }

    private void HdrToggled(object sender, RoutedEventArgs e)
    {
        _engine?.SetHdr(HdrToggle.IsOn);
        UpdateStatus();
    }

    private void ExposureChanged(object sender, RangeBaseValueChangedEventArgs e)
    {
        _engine?.SetExposure((float)e.NewValue);
    }

    private void GlowChanged(object sender, RangeBaseValueChangedEventArgs e) => ApplyColor();

    private void ColourWellClicked(object sender, RoutedEventArgs e)
    {
        var picker = new ColorPicker
        {
            Color = ColorPicker.Color,
            IsAlphaEnabled = true,
            IsColorSliderVisible = true,
            IsColorChannelTextInputVisible = false,
            IsHexInputVisible = true,
        };
        picker.ColorChanged += (_, _) =>
        {
            ColorPicker.Color = picker.Color;
            ApplyColor();
        };
        new Flyout { Content = picker }.ShowAt(ColourWell);
    }
}
