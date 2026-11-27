using System;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Controls.Primitives;
using Microsoft.UI.Xaml.Media;
using Windows.UI;

namespace DrawKit;

// The window shell: colour, extended range and the parts that have no better home. Pointer work
// lives in MainWindow.Canvas.cs, tool state in MainWindow.Tools.cs, the layer palette in
// MainWindow.Layers.cs, the status strip in MainWindow.StatusBar.cs.
public sealed partial class MainWindow : Window
{
    private PaintEngine? _engine;
    private string _tool = "brush";
    private float[] _color = { 0.10f, 0.09f, 0.08f, 1f };
    private double _zoom = 1;
    private double _panX;
    private double _panY;
    private bool _dirty;
    private string _documentName = "Untitled";

    public MainWindow()
    {
        InitializeComponent();
        Closed += (_, _) =>
        {
            CancelFillAnimation();
            if (Root.XamlRoot is not null) Root.XamlRoot.Changed -= OnXamlRootChanged;
            _engine?.Dispose();
        };
    }

    private void OnLoaded(object sender, RoutedEventArgs e)
    {
        _engine = new PaintEngine(1024, 1024);
        CanvasImage.Source = _engine.Bitmap;
        AttachSurface();
        ColorPicker.Color = Color.FromArgb(255, 24, 23, 21);
        MirrorToggle.IsOn = false;
        MirrorBothToggle.IsOn = false;
        MirrorAngleSlider.Value = 90;
        HdrToggle.IsOn = _engine.IsHdr;
        ExposureSlider.Value = _engine.Exposure;
        SnapToggle.IsOn = _engine.FillSnap;
        _engine.SetEraserStrength((float)EraserOpacitySlider.Value);
        ApplyColor();
        SyncToolButtons();
        SyncToolOptions();
        UpdateToolReadouts();
        RefreshLayers();
        Root.Focus(FocusState.Programmatic);
        if (Root.XamlRoot is not null) Root.XamlRoot.Changed += OnXamlRootChanged;
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

    private void GlowChanged(object sender, RangeBaseValueChangedEventArgs e) => ApplyColor();

    private void HdrToggled(object sender, RoutedEventArgs e)
    {
        _engine?.SetHdr(HdrToggle.IsOn);
        UpdateStatus();
    }

    private void ExposureChanged(object sender, RangeBaseValueChangedEventArgs e)
    {
        _engine?.SetExposure((float)e.NewValue);
    }

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
