//! The cut list: every part as the rectangle a cutting service takes, grouped by material
//! and size, plus a rough estimate of how much stock that needs.

use crate::material::{Material, MaterialId, MaterialKind};
use crate::model::Document;
use crate::solve::Solved;

/// Parts of the same material and size.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub material: MaterialId,
    pub quantity: usize,
    /// The longest side.
    pub length: f64,
    /// For boards: the other side of the face. For sticks: the wider side of the section.
    pub width: f64,
    pub thickness: f64,
    /// Names of the parts, in document order.
    pub names: Vec<String>,
    /// Some of these parts have a free-form outline to work after the cut.
    pub shaped: bool,
}

/// To the tenth of a millimetre.
fn round(v: f64) -> f64 {
    (v * 10.0).round() / 10.0
}

pub fn cut_list(doc: &Document, solved: &Solved) -> Vec<Row> {
    let mut rows: Vec<Row> = Vec::new();
    for part in &doc.parts {
        let (Some(b), Some(material)) = (solved.get(part.id), doc.material(part.material)) else {
            continue;
        };
        let mut sizes = b.size().map(round);
        let (length, width, thickness) = if material.is_board() {
            // The thickness is the fixed axis, whatever its size compared to the others.
            let normal = part.normal_axis().unwrap_or(0);
            let t = sizes[normal];
            let mut face: Vec<f64> = (0..3).filter(|&a| a != normal).map(|a| sizes[a]).collect();
            face.sort_by(|a, b| b.total_cmp(a));
            (face[0], face[1], t)
        } else {
            sizes.sort_by(|a, b| b.total_cmp(a));
            (sizes[0], sizes[1], sizes[2])
        };
        let shaped = part.shape.as_ref().is_some_and(|s| !s.is_plain());
        match rows.iter_mut().find(|r| {
            r.material == part.material
                && r.length == length
                && r.width == width
                && r.thickness == thickness
        }) {
            Some(row) => {
                row.quantity += 1;
                row.names.push(part.name.clone());
                row.shaped |= shaped;
            }
            None => rows.push(Row {
                material: part.material,
                quantity: 1,
                length,
                width,
                thickness,
                names: vec![part.name.clone()],
                shaped,
            }),
        }
    }
    // Materials in their order; within one, biggest first.
    let order = |id: MaterialId| doc.materials.iter().position(|m| m.id == id);
    rows.sort_by(|a, b| {
        order(a.material)
            .cmp(&order(b.material))
            .then(b.length.total_cmp(&a.length))
            .then(b.width.total_cmp(&a.width))
    });
    rows
}

/// Joins names for one cell: repeated ones are counted (`shelf ×3`).
pub fn join_names(names: &[String]) -> String {
    let mut counted: Vec<(&str, usize)> = Vec::new();
    for n in names {
        match counted.iter_mut().find(|(name, _)| *name == n.as_str()) {
            Some((_, count)) => *count += 1,
            None => counted.push((n, 1)),
        }
    }
    counted
        .iter()
        .map(|(n, c)| {
            if *c > 1 {
                format!("{n} ×{c}")
            } else {
                n.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn csv_field(text: &str) -> String {
    if text.contains([',', '"', '\n', ';']) {
        format!("\"{}\"", text.replace('"', "\"\""))
    } else {
        text.to_string()
    }
}

/// The list as CSV with the given column titles (material, quantity, length, width,
/// thickness, parts).
pub fn to_csv(doc: &Document, rows: &[Row], headers: [&str; 6]) -> String {
    let mut out = headers.map(csv_field).join(",");
    out.push('\n');
    for r in rows {
        let material = doc.material(r.material).map_or("", |m| m.name.as_str());
        let fields = [
            csv_field(material),
            r.quantity.to_string(),
            crate::expr::format_number(r.length),
            crate::expr::format_number(r.width),
            crate::expr::format_number(r.thickness),
            csv_field(&join_names(&r.names)),
        ];
        out.push_str(&fields.join(","));
        out.push('\n');
    }
    out
}

/// How much of a material the list needs.
#[derive(Clone, Debug, PartialEq)]
pub struct StockNeed {
    pub material: MaterialId,
    /// Number of pieces to cut.
    pub pieces: usize,
    /// Total area (boards, m²) or length (sticks, m).
    pub total: f64,
    /// Sheets or sticks to buy; `None` if the stock size is not given.
    pub units: Option<usize>,
    /// Pieces bigger than the stock.
    pub too_big: usize,
}

/// Estimate per material. For boards it packs the pieces in rows across the sheet (a simple
/// shelf heuristic, turning pieces freely); for sticks, first fit of the longest first. It
/// is an estimate to buy material, not a cutting plan.
pub fn stock_needs(doc: &Document, rows: &[Row]) -> Vec<StockNeed> {
    doc.materials
        .iter()
        .filter_map(|m| {
            let mine: Vec<&Row> = rows.iter().filter(|r| r.material == m.id).collect();
            if mine.is_empty() {
                return None;
            }
            Some(need(m, &mine, doc.settings.kerf))
        })
        .collect()
}

fn need(material: &Material, rows: &[&Row], kerf: f64) -> StockNeed {
    let pieces = rows.iter().map(|r| r.quantity).sum();
    match material.kind {
        MaterialKind::Board {
            stock_length,
            stock_width,
            ..
        } => {
            let total = rows
                .iter()
                .map(|r| r.length * r.width * r.quantity as f64)
                .sum::<f64>()
                / 1e6;
            let (units, too_big) = match (stock_length, stock_width) {
                (Some(l), Some(w)) if l > 0.0 && w > 0.0 => {
                    let (l, w) = (l.max(w), l.min(w));
                    let mut rects = Vec::new();
                    let mut too_big = 0;
                    for r in rows {
                        if r.length > l || r.width > w {
                            too_big += r.quantity;
                        } else {
                            rects.extend(std::iter::repeat_n((r.length, r.width), r.quantity));
                        }
                    }
                    (Some(pack_sheets(rects, l, w, kerf)), too_big)
                }
                _ => (None, 0),
            };
            StockNeed {
                material: material.id,
                pieces,
                total,
                units,
                too_big,
            }
        }
        MaterialKind::Stick { stock_length, .. } => {
            let total = rows
                .iter()
                .map(|r| r.length * r.quantity as f64)
                .sum::<f64>()
                / 1e3;
            let (units, too_big) = match stock_length {
                Some(l) if l > 0.0 => {
                    let mut lengths = Vec::new();
                    let mut too_big = 0;
                    for r in rows {
                        if r.length > l {
                            too_big += r.quantity;
                        } else {
                            lengths.extend(std::iter::repeat_n(r.length, r.quantity));
                        }
                    }
                    (Some(pack_sticks(lengths, l, kerf)), too_big)
                }
                _ => (None, 0),
            };
            StockNeed {
                material: material.id,
                pieces,
                total,
                units,
                too_big,
            }
        }
    }
}

/// First fit decreasing: number of sticks of `stock` needed for the lengths.
fn pack_sticks(mut lengths: Vec<f64>, stock: f64, kerf: f64) -> usize {
    lengths.sort_by(|a, b| b.total_cmp(a));
    let mut left: Vec<f64> = Vec::new();
    for l in lengths {
        match left.iter_mut().find(|free| **free >= l) {
            Some(free) => *free -= l + kerf,
            None => left.push(stock - l - kerf),
        }
    }
    left.len()
}

/// Shelf packing: pieces (length ≥ width, both within the sheet) in rows across the sheet.
/// Returns the number of sheets used.
fn pack_sheets(mut rects: Vec<(f64, f64)>, length: f64, width: f64, kerf: f64) -> usize {
    struct Shelf {
        height: f64,
        free: f64,
    }
    struct Sheet {
        shelves: Vec<Shelf>,
        /// Sheet width not yet taken by shelves.
        free: f64,
    }
    // Tallest rows first; a piece lies along the sheet with its short side as row height.
    rects.sort_by(|a, b| b.1.total_cmp(&a.1).then(b.0.total_cmp(&a.0)));
    let mut sheets: Vec<Sheet> = Vec::new();
    'pieces: for (l, w) in rects {
        // Ways to lay it: (along the row, row height).
        let ways = [(l, w), (w, l)];
        for sheet in &mut sheets {
            for shelf in &mut sheet.shelves {
                if let Some((along, _)) = ways
                    .iter()
                    .find(|(along, h)| *h <= shelf.height && *along <= shelf.free)
                {
                    shelf.free -= along + kerf;
                    continue 'pieces;
                }
            }
            if let Some((along, h)) = ways
                .iter()
                .find(|(along, h)| *h <= sheet.free && *along <= length)
            {
                sheet.free -= h + kerf;
                sheet.shelves.push(Shelf {
                    height: *h,
                    free: length - along - kerf,
                });
                continue 'pieces;
            }
        }
        sheets.push(Sheet {
            shelves: vec![Shelf {
                height: w,
                free: length - l - kerf,
            }],
            free: width - w - kerf,
        });
    }
    sheets.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::front::{Cota, divide, find_cell, new_frame};
    use crate::model::{Dim, End, Extent, Side};
    use crate::solve::solve;

    fn shelf_unit() -> Document {
        let mut doc = Document::new();
        doc.materials[0].name = "Melamine 18".into();
        new_frame(
            &mut doc,
            800.0,
            600.0,
            300.0,
            ["left", "right", "bottom", "top"],
        );
        let s = solve(&doc);
        let cell = find_cell(&doc, &s, 400.0, 300.0).unwrap();
        let cota = Cota {
            from_hi: false,
            ref_side: Side::Max,
            align: Side::Min,
            distance: Dim::num(273.0),
        };
        divide(&mut doc, &cell, 1, &cota, "shelf");
        doc
    }

    #[test]
    fn groups_equal_parts() {
        let doc = shelf_unit();
        let rows = cut_list(&doc, &solve(&doc));
        assert_eq!(rows.len(), 2);
        assert_eq!(
            (
                rows[0].quantity,
                rows[0].length,
                rows[0].width,
                rows[0].thickness
            ),
            (3, 764.0, 300.0, 18.0)
        );
        assert_eq!(rows[0].names, ["bottom", "top", "shelf"]);
        assert_eq!(
            (
                rows[1].quantity,
                rows[1].length,
                rows[1].width,
                rows[1].thickness
            ),
            (2, 600.0, 300.0, 18.0)
        );
        assert_eq!(join_names(&rows[1].names), "left, right");
        assert_eq!(join_names(&["a".into(), "b".into(), "a".into()]), "a ×2, b");
    }

    #[test]
    fn csv_output() {
        let doc = shelf_unit();
        let rows = cut_list(&doc, &solve(&doc));
        let csv = to_csv(
            &doc,
            &rows,
            ["Material", "Qty", "Length", "Width", "Thickness", "Parts"],
        );
        assert_eq!(
            csv,
            "Material,Qty,Length,Width,Thickness,Parts\n\
             Melamine 18,3,764,300,18,\"bottom, top, shelf\"\n\
             Melamine 18,2,600,300,18,\"left, right\"\n"
        );
    }

    #[test]
    fn sticks_list_their_length_and_section() {
        let mut doc = Document::new();
        let pine = doc.add_material(
            "Pine 2x3".into(),
            MaterialKind::Stick {
                width: 41.0,
                height: 65.0,
                stock_length: Some(3200.0),
            },
        );
        for k in 0..5 {
            doc.add_part(
                format!("leg {k}"),
                pine,
                [
                    Extent::Thick {
                        anchor: End::abs(k as f64 * 100.0),
                        align: Side::Min,
                    },
                    Extent::Span {
                        lo: End::abs(0.0),
                        hi: End::abs(700.0),
                    },
                    Extent::Thick {
                        anchor: End::abs(0.0),
                        align: Side::Min,
                    },
                ],
            );
        }
        let rows = cut_list(&doc, &solve(&doc));
        assert_eq!(rows.len(), 1);
        assert_eq!(
            (
                rows[0].quantity,
                rows[0].length,
                rows[0].width,
                rows[0].thickness
            ),
            (5, 700.0, 65.0, 41.0)
        );
        let needs = stock_needs(&doc, &rows);
        // 4 × (700 + 3) fit in 3200; the fifth needs a second stick.
        assert_eq!(needs[0].units, Some(2));
        assert_eq!(needs[0].total, 3.5);
    }

    #[test]
    fn board_estimate() {
        let mut doc = shelf_unit();
        let rows = cut_list(&doc, &solve(&doc));
        assert_eq!(stock_needs(&doc, &rows)[0].units, None);
        if let MaterialKind::Board {
            stock_length,
            stock_width,
            ..
        } = &mut doc.materials[0].kind
        {
            *stock_length = Some(2440.0);
            *stock_width = Some(1220.0);
        }
        let need = &stock_needs(&doc, &rows)[0];
        assert_eq!((need.units, need.too_big, need.pieces), (Some(1), 0, 5));
        assert!((need.total - (3.0 * 0.764 * 0.3 + 2.0 * 0.6 * 0.3)).abs() < 1e-9);

        // A sheet barely bigger than one piece: one piece per sheet.
        if let MaterialKind::Board {
            stock_length,
            stock_width,
            ..
        } = &mut doc.materials[0].kind
        {
            *stock_length = Some(770.0);
            *stock_width = Some(310.0);
        }
        assert_eq!(stock_needs(&doc, &rows)[0].units, Some(5));

        // Smaller than the big pieces.
        if let MaterialKind::Board { stock_length, .. } = &mut doc.materials[0].kind {
            *stock_length = Some(700.0);
        }
        let need = &stock_needs(&doc, &rows)[0];
        assert_eq!((need.units, need.too_big), (Some(2), 3));
    }
}
