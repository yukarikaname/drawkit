// Pointer handling. Coordinates go to the core in document pixels; pan and zoom stay in screen
// space as a render transform, and the GPU surface is sized in physical pixels so a 200% display
// draws at 200%. Pen pressure, tilt and twist come straight from PointerPointProperties.

using System;
using Microsoft.UI.Input;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Input;
using Microsoft.UI.Xaml.Media;
using Windows.Foundation;
using Windows.System;
using Windows.UI;
using Windows.UI.Core;

namespace DrawKit;

public sealed partial class MainWindow
{
    private bool _drawing;
    private double _lastX;
    private double _lastY;
    private bool _panning;
    private Point _panStart;
    private string? _toolBeforeBarrel;

    private static bool SpaceDown =>
        (InputKeyboardSource.GetKeyStateForCurrentThread(VirtualKey.Space)
            & CoreVirtualKeyStates.Down) != 0;

    private void CanvasPressed(object sender, PointerRoutedEventArgs e)
    {
        if (_engine is null) return;
        Root.Focus(FocusState.Programmatic);
        var current = e.GetCurrentPoint(CanvasHost);
        var properties = current.Properties;

        // The pen reports its own tip before any button: the eraser tip erases, the barrel samples
        // a colour while it is held and hands the previous tool back on release.
        if (properties.IsEraser)
        {
            SelectTool("eraser");
        }
        else if (properties.IsBarrelButtonPressed && _tool != "eyedropper")
        {
            _toolBeforeBarrel = _tool;
            SelectTool("eyedropper");
        }

        if (SpaceDown || properties.IsMiddleButtonPressed ||
            (properties.IsRightButtonPressed && !properties.IsBarrelButtonPressed))
        {
            _panning = true;
            _panStart = current.Position;
            CanvasHost.CapturePointer(e.Pointer);
            return;
        }
        var document = ToDocument(current.Position);
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
        _engine.Begin(_tool == "eraser", AutoShapeToggle.IsOn);
        var start = PenDynamics(current);
        _engine.Segment(document.X, document.Y, document.X, document.Y,
            ActiveRadius, Pressure(current),
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
            ActiveRadius, Pressure(point),
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
        if (_drawing)
        {
            _drawing = false;
            CanvasHost.ReleasePointerCapture(e.Pointer);
            _engine?.End();
            _engine?.AutoShape();
            MarkDirty();
            RefreshLayers();
        }
        // A barrel sample is a momentary tool, not a mode: hand the previous tool back even when
        // the press sampled instead of drawing.
        if (_toolBeforeBarrel is not null)
        {
            SelectTool(_toolBeforeBarrel);
            _toolBeforeBarrel = null;
        }
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

    /// A window dragged to a display with a different scale needs a new swapchain size, or the
    /// surface keeps the old pixel density.
    private void OnXamlRootChanged(XamlRoot sender, XamlRootChangedEventArgs args)
    {
        ApplyTransform();
        ResizeSurface();
    }

    private double RasterScale => CanvasHost.XamlRoot?.RasterizationScale ?? 1;
    private uint _surfacePixelWidth;
    private uint _surfacePixelHeight;

    private void AttachSurface()
    {
        if (_engine is null) return;
        var width = (uint)Math.Max(1, CanvasHost.ActualWidth * RasterScale);
        var height = (uint)Math.Max(1, CanvasHost.ActualHeight * RasterScale);
        if (_engine.AttachSwapChainPanel(CanvasSwapChain, width, height))
        {
            _surfacePixelWidth = width;
            _surfacePixelHeight = height;
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
        // XamlRoot.Changed fires for more than a scale change; a swapchain resize for the same
        // physical size is pure cost.
        if (width == _surfacePixelWidth && height == _surfacePixelHeight) return;
        _surfacePixelWidth = width;
        _surfacePixelHeight = height;
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

    /// One document pixel per screen pixel. The base scale always fits the paper, so the zoom
    /// that cancels it is the reciprocal of the fit scale.
    private void ActualSize()
    {
        if (_engine is null) return;
        var fit = Math.Min(CanvasHost.ActualWidth / _engine.Width,
                           CanvasHost.ActualHeight / _engine.Height);
        if (fit <= 0) return;
        _zoom = Math.Clamp(1 / fit, 0.1, 32);
        _panX = 0;
        _panY = 0;
        ApplyTransform();
        UpdateStatus();
    }

    private void ZoomBy(double factor)
    {
        if (CanvasHost.ActualWidth <= 0 || CanvasHost.ActualHeight <= 0) return;
        var centre = new Point(CanvasHost.ActualWidth / 2, CanvasHost.ActualHeight / 2);
        ZoomAt(centre, factor);
    }

    private void ZoomAt(Point point, double factor)
    {
        var old = _zoom;
        _zoom = Math.Clamp(_zoom * factor, 0.1, 32);
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
}
