package dev.drawkit.android.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Add
import androidx.compose.material.icons.rounded.AutoAwesome
import androidx.compose.material.icons.rounded.Close
import androidx.compose.material.icons.rounded.CreateNewFolder
import androidx.compose.material.icons.rounded.FormatColorFill
import androidx.compose.material.icons.rounded.KeyboardArrowDown
import androidx.compose.material.icons.automirrored.rounded.KeyboardArrowRight
import androidx.compose.material.icons.rounded.Remove
import androidx.compose.material.icons.rounded.MoreVert
import androidx.compose.material.icons.rounded.Visibility
import androidx.compose.material.icons.rounded.VisibilityOff
import androidx.compose.material3.BottomSheetDefaults
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Slider
import androidx.compose.material3.Surface
import androidx.compose.material3.SwipeToDismissBox
import androidx.compose.material3.SwipeToDismissBoxValue
import androidx.compose.material3.rememberSwipeToDismissBoxState
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.foundation.layout.Box
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.foundation.Image
import androidx.compose.foundation.clickable
import dev.drawkit.android.core.LayerRow
import dev.drawkit.android.core.PaintState

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun LayersSheet(state: PaintState, onDismiss: () -> Unit) {
    val sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true)
    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = sheetState,
        containerColor = MaterialTheme.colorScheme.surface,
        contentColor = MaterialTheme.colorScheme.onSurface,
        dragHandle = { BottomSheetDefaults.DragHandle(color = MaterialTheme.colorScheme.onSurfaceVariant) },
    ) {
        LayersPanel(state, onClose = onDismiss,
            modifier = Modifier.padding(horizontal = 18.dp).padding(bottom = 24.dp)
                .navigationBarsPadding().safeHorizontalInsets())
    }
}

/** A floating layer panel for tablet widths; the sheet remains the compact form. */
@Composable
fun LayerDock(state: PaintState, modifier: Modifier = Modifier) {
    Surface(
        modifier = modifier,
        shape = RoundedCornerShape(20.dp),
        color = MaterialTheme.colorScheme.surface,
        border = androidx.compose.foundation.BorderStroke(1.dp, Color.White.copy(alpha = 0.07f)),
        shadowElevation = 14.dp,
    ) {
        LayersPanel(state, onClose = null, modifier = Modifier.padding(14.dp))
    }
}

@Composable
private fun LayersPanel(
    state: PaintState,
    onClose: (() -> Unit)?,
    modifier: Modifier = Modifier,
) {
    var query by remember { mutableStateOf("") }
    var renaming by remember { mutableStateOf<LayerRow?>(null) }
    var renameText by remember { mutableStateOf("") }
    Column(modifier = modifier.fillMaxWidth()) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            BrandMark(size = 24.dp)
            Spacer(Modifier.width(10.dp))
            Text("Layers", style = MaterialTheme.typography.headlineSmall)
            Spacer(Modifier.weight(1f))
            Glyph(Icons.Rounded.Add, "Add layer") {
                state.addLayer()
                state.refreshThumbnails()
            }
            Glyph(Icons.Rounded.CreateNewFolder, "Add group") {
                state.addGroup()
                state.refreshThumbnails()
            }
            Glyph(Icons.Rounded.FormatColorFill, "Add fill layer") {
                state.addFillLayer()
                state.refreshThumbnails()
            }
            Glyph(Icons.Rounded.AutoAwesome, "Extract line art") {
                state.extractLineArt()
            }
            Glyph(Icons.Rounded.Remove, "Remove layer") {
                state.removeLayer()
                state.refreshThumbnails()
            }
            if (onClose != null) {
                Glyph(Icons.Rounded.Close, "Close") { onClose() }
            }
        }
        Spacer(Modifier.height(8.dp))
        OutlinedTextField(
            value = query,
            onValueChange = { query = it },
            label = { Text("Search layers") },
            singleLine = true,
            modifier = Modifier.fillMaxWidth(),
        )
        Spacer(Modifier.height(6.dp))

        LazyColumn(
            modifier = Modifier.heightIn(max = 360.dp),
            verticalArrangement = Arrangement.spacedBy(4.dp),
        ) {
            items(state.layerRows().filter { query.isBlank() || it.name.contains(query, ignoreCase = true) }, key = { it.index }) { layer ->
                LayerRowView(state, layer) {
                    renameText = layer.name
                    renaming = layer
                }
            }
        }

        val active = state.layers.firstOrNull { it.index == state.activeLayer }
        if (active != null) {
            Spacer(Modifier.height(10.dp))
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text("Opacity", color = MaterialTheme.colorScheme.onSurfaceVariant, fontSize = 11.sp)
                Spacer(Modifier.weight(1f))
                Text("${(active.opacity * 100).toInt()}%", color = MaterialTheme.colorScheme.onSurfaceVariant, fontSize = 11.sp)
            }
            Slider(
                value = active.opacity,
                onValueChange = { state.setOpacity(active.index, it) },
                valueRange = 0f..1f,
                colors = brandSliderColors(),
            )
        }
    }
    if (renaming != null) {
        AlertDialog(
            onDismissRequest = { renaming = null },
            title = { Text("Rename layer") },
            text = {
                OutlinedTextField(
                    value = renameText,
                    onValueChange = { renameText = it },
                    singleLine = true,
                )
            },
            confirmButton = {
                TextButton(onClick = {
                    val name = renameText.trim()
                    if (name.isNotEmpty()) state.renameLayer(renaming!!.index, name)
                    renaming = null
                }) { Text("Rename") }
            },
            dismissButton = { TextButton(onClick = { renaming = null }) { Text("Cancel") } },
        )
    }
}

@Composable
private fun tagColor(tag: Int): Color = when (tag) {
    1 -> MaterialTheme.colorScheme.primary
    2 -> Color(0.95f, 0.62f, 0.20f)
    3 -> Color(0.95f, 0.85f, 0.25f)
    4 -> Color(0.30f, 0.70f, 0.35f)
    5 -> Color(0.25f, 0.55f, 0.90f)
    6 -> Color(0.55f, 0.35f, 0.80f)
    else -> Color.Transparent
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun LayerRowView(state: PaintState, layer: LayerRow, onRename: () -> Unit) {
    val selected = layer.index == state.activeLayer
    var menu by remember { mutableStateOf(false) }
    val dismiss = rememberSwipeToDismissBoxState()
    LaunchedEffect(dismiss.currentValue) {
        if (dismiss.currentValue == SwipeToDismissBoxValue.EndToStart) {
            state.selectLayer(layer.index)
            state.removeLayer()
            state.refreshThumbnails()
        }
    }
    SwipeToDismissBox(
        state = dismiss,
        enableDismissFromStartToEnd = false,
        backgroundContent = {
            Box(
                Modifier
                    .fillMaxWidth()
                    .height(58.dp)
                    .background(MaterialTheme.colorScheme.primary, RoundedCornerShape(12.dp))
                    .padding(horizontal = 18.dp),
                contentAlignment = Alignment.CenterEnd,
            ) {
                Text("Delete", color = MaterialTheme.colorScheme.onPrimary)
            }
        },
    ) {
    Surface(
        modifier = Modifier.fillMaxWidth(),
        shape = RoundedCornerShape(12.dp),
        color = if (selected) MaterialTheme.colorScheme.primary.copy(alpha = 0.18f) else Color.Transparent,
        onClick = { state.selectLayer(layer.index) },
    ) {
        Row(
            modifier = Modifier
                .height(58.dp)
                .padding(start = 8.dp + (layer.depth * 14).dp, end = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            if (layer.isGroup) {
                Box(
                    modifier = Modifier
                        .size(24.dp)
                        .clip(RoundedCornerShape(6.dp))
                        .clickable { state.setExpanded(layer.index, !layer.expanded) },
                    contentAlignment = Alignment.Center,
                ) {
                    Icon(
                        imageVector = if (layer.expanded) Icons.Rounded.KeyboardArrowDown
                                      else Icons.AutoMirrored.Rounded.KeyboardArrowRight,
                        contentDescription = if (layer.expanded) "Collapse group" else "Open group",
                        tint = MaterialTheme.colorScheme.onSurfaceVariant,
                        modifier = Modifier.size(18.dp),
                    )
                }
            } else {
                Spacer(Modifier.width(24.dp))
            }
            val thumbnail = state.thumbnails[layer.index]
            Surface(
                modifier = Modifier.size(40.dp),
                shape = RoundedCornerShape(7.dp),
                color = if (layer.isGroup || layer.isFill) Color.White.copy(alpha = 0.06f)
                        else MaterialTheme.colorScheme.onSurface,
            ) {
                when {
                    layer.isGroup -> Icon(
                        Icons.Rounded.CreateNewFolder, layer.name,
                        tint = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.7f),
                        modifier = Modifier.padding(8.dp),
                    )
                    layer.isFill -> Icon(
                        Icons.Rounded.FormatColorFill, layer.name,
                        tint = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.7f),
                        modifier = Modifier.padding(8.dp),
                    )
                    thumbnail != null -> Image(
                        bitmap = thumbnail.asImageBitmap(),
                        contentDescription = layer.name,
                        modifier = Modifier.size(40.dp),
                        contentScale = ContentScale.Crop,
                    )
                }
            }
            Spacer(Modifier.width(10.dp))
            Column {
                Row(horizontalArrangement = Arrangement.spacedBy(5.dp), verticalAlignment = Alignment.CenterVertically) {
                    if (layer.reference) {
                        Icon(
                            Icons.Rounded.CreateNewFolder, "Reference layer",
                            tint = MaterialTheme.colorScheme.onSurfaceVariant,
                            modifier = Modifier.size(10.dp),
                        )
                    }
                    if (layer.tag != 0) {
                        Box(
                            Modifier
                                .size(6.dp)
                                .clip(RoundedCornerShape(3.dp))
                                .background(tagColor(layer.tag))
                        )
                    }
                    Text(
                        layer.name,
                    color = MaterialTheme.colorScheme.onSurface,
                    fontWeight = if (selected) FontWeight.SemiBold else FontWeight.Normal,
                        fontSize = 14.sp,
                    )
                }
                Text(
                    "${(layer.opacity * 100).toInt()}%",
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    fontSize = 10.sp,
                )
            }
            Spacer(Modifier.weight(1f))
            Box {
                Glyph(Icons.Rounded.MoreVert, "Layer options") { menu = true }
                DropdownMenu(expanded = menu, onDismissRequest = { menu = false }) {
                    DropdownMenuItem(text = { Text(if (layer.reference) "Reference: on" else "Reference: off") },
                        onClick = {
                            state.chooseReference(layer.index, !layer.reference)
                            menu = false
                        })
                    DropdownMenuItem(text = { Text("Colour tag") }, onClick = {})
                    val names = listOf("None", "Red", "Orange", "Yellow", "Green", "Blue", "Purple")
                    (0..6).forEach { tag ->
                        DropdownMenuItem(
                            text = { Text(names[tag]) },
                            onClick = {
                                state.chooseTag(layer.index, tag)
                                menu = false
                            },
                        )
                    }
                    DropdownMenuItem(text = { Text("Move up") }, onClick = {
                        menu = false
                        state.moveLayer(layer.index, 1)
                    })
                    DropdownMenuItem(text = { Text("Move down") }, onClick = {
                        menu = false
                        state.moveLayer(layer.index, -1)
                    })
                    DropdownMenuItem(text = { Text("Rename…") }, onClick = {
                        menu = false
                        onRename()
                    })
                    DropdownMenuItem(text = { Text("Delete") }, onClick = {
                        menu = false
                        state.selectLayer(layer.index)
                        state.removeLayer()
                        state.refreshThumbnails()
                    })
                }
            }
            Box(
                modifier = Modifier
                    .size(38.dp)
                    .clip(RoundedCornerShape(10.dp))
                    .clickable { state.setVisible(layer.index, !layer.visible) },
                contentAlignment = Alignment.Center,
            ) {
                Icon(
                    imageVector = if (layer.visible) Icons.Rounded.Visibility else Icons.Rounded.VisibilityOff,
                    contentDescription = if (layer.visible) "Hide layer" else "Show layer",
                    tint = MaterialTheme.colorScheme.onSurface.copy(alpha = if (layer.visible) 0.75f else 0.35f),
                    modifier = Modifier.size(19.dp),
                )
            }
        }
    }
    }
}
