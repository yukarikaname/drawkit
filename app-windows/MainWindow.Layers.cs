using System;
using System.Collections.Generic;
using System.Linq;
using System.Runtime.InteropServices.WindowsRuntime;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Controls.Primitives;
using Microsoft.UI.Xaml.Input;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Media.Imaging;

namespace DrawKit;

// The layer palette: the tree, the thumbnail per row, and the settings that only make sense for
// the active layer. Rows are rebuilt after every change, so nothing here keeps a stale index.
public sealed partial class MainWindow
{
    private bool _syncingLayerUi;
    private string _layerQuery = "";

    private void LayerSearchChanged(object sender, TextChangedEventArgs e)
    {
        _layerQuery = (sender as TextBox)?.Text.Trim() ?? "";
        RefreshLayers();
    }

    // A single-letter shortcut must not fire while the search field has the keyboard.
    private void LayerSearchFocused(object sender, RoutedEventArgs e) => _typing = true;
    private void LayerSearchLostFocus(object sender, RoutedEventArgs e) => _typing = false;

    private void RefreshLayers()
    {
        if (_engine is null) return;
        var nodes = _engine.Layers();
        var rows = new List<LayerRow>();
        if (_layerQuery.Length > 0)
        {
            foreach (var node in nodes
                         .Where(n => n.Name.Contains(_layerQuery,
                                                     StringComparison.OrdinalIgnoreCase))
                         .OrderByDescending(n => n.Index))
            {
                rows.Add(RowFor(node, DepthOf(nodes, node)));
            }
        }
        else
        {
            void Add(int? parent, int depth)
            {
                foreach (var node in nodes.Where(n => n.Parent == parent)
                             .OrderByDescending(n => n.Index))
                {
                    rows.Add(RowFor(node, depth));
                    if (node.Kind == 1 && node.Expanded) Add((int)node.Index, depth + 1);
                }
            }
            Add(null, 0);
        }
        LayersList.ItemsSource = rows;
        var active = rows.FirstOrDefault(row => (int)row.Index == _engine.ActiveLayer);
        if (active is not null) LayersList.SelectedItem = active;
        SyncLayerControls(active);
        UndoButton.IsEnabled = _engine.CanUndo;
        RedoButton.IsEnabled = _engine.CanRedo;
        UpdateStatus();
    }

    private LayerRow RowFor(LayerNode node, int depth)
    {
        // A thumbnail is a 28 px downscale of a layer; the core does the work, the shell only
        // encodes it once. A group has no pixels to show; a fill layer has its colour.
        var thumbnail = _engine is not null && node.Kind != 1
            ? MakeThumbnail(_engine.Thumbnail((int)node.Index, 28, 28))
            : null;        return new LayerRow(node, depth, thumbnail);
    }

    private static ImageSource? MakeThumbnail(byte[]? rgba, int size = 28)
    {
        if (rgba is null) return null;
        var bitmap = new WriteableBitmap(size, size);
        using var stream = bitmap.PixelBuffer.AsStream();
        for (var index = 0; index < rgba.Length; index += 4)
        {
            // Premultiplied RGBA to BGRA over paper, the same flattening the canvas uses.
            var alpha = rgba[index + 3];
            var paper = 255 - alpha;
            stream.WriteByte((byte)Math.Min(255, rgba[index + 2] + paper));
            stream.WriteByte((byte)Math.Min(255, rgba[index + 1] + paper));
            stream.WriteByte((byte)Math.Min(255, rgba[index] + paper));
            stream.WriteByte(255);
        }
        bitmap.Invalidate();
        return bitmap;
    }

    private static int DepthOf(List<LayerNode> nodes, LayerNode node)
    {
        var depth = 0;
        var parent = node.Parent;
        while (parent is int index && index >= 0 && index < nodes.Count)
        {
            depth++;
            parent = nodes[index].Parent;
        }
        return depth;
    }

    private void SyncLayerControls(LayerRow? row)
    {
        if (LayerOpacitySlider is null) return;
        _syncingLayerUi = true;
        LayerOpacitySlider.Value = row?.Opacity ?? 1;
        BlendCombo.SelectedIndex = row is null
            ? 0
            : Math.Clamp(row.Blend, 0, BlendCombo.Items.Count - 1);
        LayerLockToggle.IsOn = row?.Locked ?? false;
        LayerAlphaToggle.IsOn = row?.AlphaLocked ?? false;
        _syncingLayerUi = false;
    }

    private void LayerSelectionChanged(object sender, SelectionChangedEventArgs e)
    {
        if (_engine is null || LayersList.SelectedItem is not LayerRow row) return;
        _engine.SelectLayer((int)row.Index);
        SyncLayerControls(row);
        UpdateStatus();
    }

    private void LayerDoubleTapped(object sender, DoubleTappedRoutedEventArgs e)
    {
        if (_engine is null) return;
        if ((e.OriginalSource as FrameworkElement)?.DataContext is not LayerRow row) return;
        LayersList.SelectedItem = row;
        if (row.Kind == 1)
        {
            _engine.SetExpanded((int)row.Index, !row.Expanded);
            RefreshLayers();
            return;
        }
        RenameLayerClicked(this, new RoutedEventArgs());
    }

    private void VisibilityClicked(object sender, RoutedEventArgs e)
    {
        if (_engine is null || sender is not FrameworkElement element ||
            element.DataContext is not LayerRow row) return;
        _engine.SetVisible((int)row.Index, !row.Visible);
        MarkDirty();
        RefreshLayers();
    }

    private void LayerOpacityChanged(object sender, RangeBaseValueChangedEventArgs e)
    {
        if (_syncingLayerUi || _engine is null) return;
        if (LayersList.SelectedItem is not LayerRow row) return;
        _engine.SetOpacity((int)row.Index, (float)e.NewValue);
        MarkDirty();
    }

    private void BlendChanged(object sender, SelectionChangedEventArgs e)
    {
        if (_syncingLayerUi || _engine is null) return;
        if (LayersList.SelectedItem is not LayerRow row) return;
        _engine.SetBlend((int)row.Index, BlendCombo.SelectedIndex);
        MarkDirty();
        RefreshLayers();
    }

    private void LayerLockToggled(object sender, RoutedEventArgs e)
    {
        if (_syncingLayerUi || _engine is null) return;
        if (LayersList.SelectedItem is not LayerRow row) return;
        _engine.SetLocked((int)row.Index, LayerLockToggle.IsOn);
        MarkDirty();
        RefreshLayers();
    }

    private void LayerAlphaLockToggled(object sender, RoutedEventArgs e)
    {
        if (_syncingLayerUi || _engine is null) return;
        if (LayersList.SelectedItem is not LayerRow row) return;
        _engine.SetAlphaLocked((int)row.Index, LayerAlphaToggle.IsOn);
        MarkDirty();
        RefreshLayers();
    }

    private void AddLayerClicked(object sender, RoutedEventArgs e)
    {
        _engine?.AddLayer();
        MarkDirty();
        RefreshLayers();
    }

    private void DuplicateLayerClicked(object sender, RoutedEventArgs e)
    {
        if (_engine?.DuplicateLayer() != true) return;
        MarkDirty();
        RefreshLayers();
    }

    private void AddGroupClicked(object sender, RoutedEventArgs e)
    {
        _engine?.AddGroup();
        MarkDirty();
        RefreshLayers();
    }

    private void AddFillClicked(object sender, RoutedEventArgs e)
    {
        _engine?.AddFill(_color);
        MarkDirty();
        RefreshLayers();
    }

    private void ExtractLineArtClicked(object sender, RoutedEventArgs e)
    {
        if (_engine?.ExtractLineArt() != true) return;
        MarkDirty();
        RefreshLayers();
    }

    private void CycleTagClicked(object sender, RoutedEventArgs e)
    {
        if (_engine is null || LayersList.SelectedItem is not LayerRow row) return;
        _engine.SetTag((int)row.Index, (row.Tag + 1) % 7);
        MarkDirty();
        RefreshLayers();
    }

    private void RemoveLayerClicked(object sender, RoutedEventArgs e)
    {
        _engine?.RemoveLayer();
        MarkDirty();
        RefreshLayers();
    }

    private void MoveLayerUpClicked(object sender, RoutedEventArgs e) => MoveLayer(1);

    private void MoveLayerDownClicked(object sender, RoutedEventArgs e) => MoveLayer(-1);

    private void MoveLayer(int steps)
    {
        if (_engine is null || LayersList.SelectedItem is not LayerRow row) return;
        _engine.MoveLayer((int)row.Index, steps);
        MarkDirty();
        RefreshLayers();
    }

    private async void RenameLayerClicked(object sender, RoutedEventArgs e)
    {
        if (_engine is null || LayersList.SelectedItem is not LayerRow row) return;
        var field = new TextBox { Text = row.Name, SelectionStart = row.Name.Length };
        var dialog = new ContentDialog
        {
            Title = "Rename layer",
            Content = field,
            PrimaryButtonText = "Rename",
            CloseButtonText = "Cancel",
            DefaultButton = ContentDialogButton.Primary,
            XamlRoot = Root.XamlRoot,
        };
        if (await dialog.ShowAsync() != ContentDialogResult.Primary) return;
        var name = field.Text.Trim();
        if (name.Length == 0) return;
        _engine.RenameLayer((int)row.Index, name);
        MarkDirty();
        RefreshLayers();
    }
}
