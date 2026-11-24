using System;
using System.Collections.Generic;
using System.Linq;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Controls.Primitives;

namespace DrawKit;

public sealed partial class MainWindow
{
    private void LayerSearchChanged(object sender, TextChangedEventArgs e)
    {
        _layerQuery = LayerSearch.Text.Trim();
        RefreshLayers();
    }

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
                rows.Add(new LayerRow(node, DepthOf(nodes, node)));
            }
        }
        else
        {
            void Add(int? parent, int depth)
            {
                foreach (var node in nodes.Where(n => n.Parent == parent)
                             .OrderByDescending(n => n.Index))
                {
                    rows.Add(new LayerRow(node, depth));
                    if (node.Kind == 1 && node.Expanded) Add((int)node.Index, depth + 1);
                }
            }
            Add(null, 0);
        }
        LayersList.ItemsSource = rows;
        var active = rows.FirstOrDefault(row => (int)row.Index == _engine.ActiveLayer);
        if (active is not null)
        {
            LayersList.SelectedItem = active;
            LayerOpacitySlider.Value = active.Opacity;
        }
        UndoButton.IsEnabled = _engine.CanUndo;
        RedoButton.IsEnabled = _engine.CanRedo;
        UpdateStatus();
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

    private void LayerSelectionChanged(object sender, SelectionChangedEventArgs e)
    {
        if (_engine is null || LayersList.SelectedItem is not LayerRow row) return;
        _engine.SelectLayer((int)row.Index);
        LayerOpacitySlider.Value = row.Opacity;
        UpdateStatus();
    }

    private void VisibilityClicked(object sender, RoutedEventArgs e)
    {
        if (_engine is null || sender is not FrameworkElement element ||
            element.DataContext is not LayerRow row) return;
        _engine.SetVisible((int)row.Index, !row.Visible);
        RefreshLayers();
    }

    private void LayerOpacityChanged(object sender, RangeBaseValueChangedEventArgs e)
    {
        if (_engine is null || LayersList.SelectedItem is not LayerRow row) return;
        _engine.SetOpacity((int)row.Index, (float)e.NewValue);
    }

    private void AddLayerClicked(object sender, RoutedEventArgs e)
    {
        _engine?.AddLayer();
        RefreshLayers();
    }

    private void AddGroupClicked(object sender, RoutedEventArgs e)
    {
        _engine?.AddGroup();
        RefreshLayers();
    }

    private void AddFillClicked(object sender, RoutedEventArgs e)
    {
        _engine?.AddFill(_color);
        RefreshLayers();
    }

    private void RemoveLayerClicked(object sender, RoutedEventArgs e)
    {
        _engine?.RemoveLayer();
        RefreshLayers();
    }

    private void MoveLayerUpClicked(object sender, RoutedEventArgs e) => MoveLayer(1);

    private void MoveLayerDownClicked(object sender, RoutedEventArgs e) => MoveLayer(-1);

    private void MoveLayer(int steps)
    {
        if (_engine is null || LayersList.SelectedItem is not LayerRow row) return;
        _engine.MoveLayer((int)row.Index, steps);
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
        RefreshLayers();
    }
}
