using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Media;
using Windows.UI;

namespace DrawKit;

/// One row in the layer list. Rebuilt whenever the engine reports a layer change, so the bindings
/// stay one-way and small.
public sealed class LayerRow
{
    public LayerRow(LayerNode node, int depth)
    {
        Index = node.Index;
        Name = node.Name;
        Visible = node.Visible;
        Opacity = node.Opacity;
        Kind = node.Kind;
        Depth = depth;
        Expanded = node.Expanded;
        Tag = node.Tag;
        TagBrush = new SolidColorBrush(TagColor(node.Tag));
    }

    public uint Index { get; }
    public string Name { get; }
    public bool Visible { get; }
    public double Opacity { get; }
    public int Kind { get; }
    public int Depth { get; }
    public bool Expanded { get; }
    public int Tag { get; }
    public SolidColorBrush TagBrush { get; }
    public string TagGlyph => Tag == 0 ? "" : "●";

    public bool IsGroup => Kind == 1;
    public string Glyph => Kind == 1 ? "\uE8B7" : Kind == 2 ? "\uE790" : "\uE70F";
    public string OpacityText => $"{Opacity * 100:0}%";
    public Thickness Indent => new(Depth * 14, 0, 0, 0);

    private static Color TagColor(int tag) => tag switch
    {
        1 => Color.FromArgb(255, 197, 82, 61),
        2 => Color.FromArgb(255, 242, 158, 51),
        3 => Color.FromArgb(255, 242, 217, 64),
        4 => Color.FromArgb(255, 77, 179, 89),
        5 => Color.FromArgb(255, 64, 140, 230),
        6 => Color.FromArgb(255, 140, 89, 204),
        _ => Color.FromArgb(0, 0, 0, 0),
    };
}
