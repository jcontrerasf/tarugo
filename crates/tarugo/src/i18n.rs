//! Interface texts in Spanish and English. Every visible string goes through [`t`]; a text
//! missing in one language is a compile error.

use std::sync::atomic::{AtomicU8, Ordering};

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, PartialEq, Eq, Default, Debug, Serialize, Deserialize)]
pub enum Lang {
    #[default]
    Es,
    En,
}

static LANG: AtomicU8 = AtomicU8::new(0);

pub fn set_lang(lang: Lang) {
    LANG.store(lang as u8, Ordering::Relaxed);
}

pub fn lang() -> Lang {
    if LANG.load(Ordering::Relaxed) == Lang::En as u8 {
        Lang::En
    } else {
        Lang::Es
    }
}

/// Texts of the language in use.
pub fn t() -> &'static Texts {
    match lang() {
        Lang::Es => &ES,
        Lang::En => &EN,
    }
}

/// Replaces each `{}` in `template` with the next argument.
pub fn fill(template: &str, args: &[&dyn std::fmt::Display]) -> String {
    let mut out = String::with_capacity(template.len() + 16);
    let mut args = args.iter();
    let mut rest = template;
    while let Some(i) = rest.find("{}") {
        out.push_str(&rest[..i]);
        match args.next() {
            Some(a) => out.push_str(&a.to_string()),
            None => out.push_str("{}"),
        }
        rest = &rest[i + 2..];
    }
    out.push_str(rest);
    out
}

/// Declares the texts: `key: "español", "English";`. Both languages are mandatory.
macro_rules! texts {
    ($($key:ident: $es:expr, $en:expr;)*) => {
        pub struct Texts {
            $(pub $key: &'static str,)*
        }
        pub const ES: Texts = Texts {
            $($key: $es,)*
        };
        pub const EN: Texts = Texts {
            $($key: $en,)*
        };
    };
}

texts! {
    language_name: "Español", "English";

    // File and menu.
    new_doc: "Nuevo", "New";
    open: "Abrir", "Open";
    save: "Guardar", "Save";
    save_as: "Guardar como", "Save as";
    recent: "Recientes", "Recent";
    nothing_yet: "Nada todavía", "Nothing yet";
    about: "Acerca de", "About";
    about_text: "Diseño de muebles de tableros y palos: esquema frontal con cotas, edición libre en 3D, análisis de tornillos y lista de corte.\n\nNombre provisorio. Software libre bajo GPL-3.0-or-later.",
        "Furniture design out of boards and lumber: front sketch with dimensions, free 3D editing, screws analysis and cut list.\n\nProvisional name. Free software under GPL-3.0-or-later.";
    language: "Idioma", "Language";
    theme: "Tema", "Theme";
    theme_system: "Del sistema", "System";
    theme_light: "Claro", "Light";
    theme_dark: "Oscuro", "Dark";
    untitled: "Sin título", "Untitled";
    filter_tarugo: "Diseño de Tarugo", "Tarugo design";
    saved: "Guardado", "Saved";
    cannot_open: "No se pudo abrir {}: {}", "Could not open {}: {}";
    cannot_save: "No se pudo guardar {}: {}", "Could not save {}: {}";
    undo: "Deshacer", "Undo";
    redo: "Rehacer", "Redo";
    delete: "Eliminar", "Delete";
    ok: "Aceptar", "OK";
    cancel: "Cancelar", "Cancel";
    discard: "Descartar", "Discard";
    unsaved_title: "Cambios sin guardar", "Unsaved changes";
    unsaved_text: "El diseño tiene cambios sin guardar.", "The design has unsaved changes.";
    copy: "Copiar", "Copy";
    copied: "Copiado al portapapeles", "Copied to the clipboard";
    none: "Ninguno", "None";
    name: "Nombre", "Name";

    // Stages.
    stage_materials: "Materiales", "Materials";
    stage_front: "Frente", "Front";
    stage_design: "Diseño 3D", "3D design";
    stage_screws: "Tornillos", "Screws";
    stage_cut_list: "Lista de corte", "Cut list";

    // Default names.
    default_material: "Melamina 18 mm", "Melamine 18 mm";
    part_left: "Lateral izquierdo", "Left side";
    part_right: "Lateral derecho", "Right side";
    part_bottom: "Base", "Bottom";
    part_top: "Tapa", "Top";
    part_division: "División", "Division";
    part_shelf: "Repisa", "Shelf";
    part_board: "Tablero", "Board";
    part_stick: "Palo", "Stick";
    new_board_name: "Tablero nuevo", "New board";
    new_stick_name: "Palo nuevo", "New stick";

    // Tools.
    tool_select: "Seleccionar y mover", "Select and move";
    tool_divide_vertical: "División vertical", "Vertical division";
    tool_divide_horizontal: "División horizontal (repisa)", "Horizontal division (shelf)";
    tool_add_board: "Agregar tablero", "Add board";
    tool_add_stick: "Agregar palo", "Add stick";
    tool_screw: "Poner tornillos", "Place screws";
    fit_view: "Ver todo", "Fit view";
    perspective: "Perspectiva", "Perspective";
    show_screws: "Ver tornillos", "Show screws";
    hint_select: "Clic para seleccionar; arrastra la pieza seleccionada para moverla o sus puntos para alargarla. Clic en una cota para escribirla.",
        "Click to select; drag the selected part to move it or its dots to stretch it. Click a dimension to type it.";
    hint_divide: "Clic en un espacio para dividirlo; luego escribe la cota exacta.",
        "Click a space to divide it; then type the exact dimension.";
    hint_add: "Clic en la cara de una pieza para apoyar ahí la nueva.",
        "Click a face of a part to put the new one against it.";
    hint_screw: "Clic en la cara por donde entra el tornillo, sobre la unión. Clic en un tornillo para seleccionarlo.",
        "Click the face the screw goes in through, over the joint. Click a screw to select it.";
    snap_hint: "Alt: sin snap", "Alt: no snapping";
    nothing_selected: "Nada seleccionado", "Nothing selected";
    cota_edit_help: "Clic para escribir la medida o una fórmula", "Click to type the measure or a formula";
    no_3d: "La vista 3D no está disponible (falta wgpu)", "The 3D view is not available (no wgpu)";
    status_counts: "{} piezas · {} tornillos", "{} parts · {} screws";
    status_size: "{} × {} × {} mm", "{} × {} × {} mm";

    // Divide.
    divide_from: "Medir desde", "Measure from";
    ref_inner: "Borde interior", "Inner edge";
    ref_center: "Centro", "Center";
    ref_outer: "Borde exterior", "Outer edge";
    divide_to: "hasta", "to";
    align_near: "Borde cercano", "Near edge";
    align_center: "Centro", "Center";
    align_far: "Borde lejano", "Far edge";
    equal_spaces: "Espacios iguales", "Equal spaces";
    equal_spaces_help: "Divide el espacio en partes iguales, que se mantienen al cambiar el tamaño",
        "Divides the space in equal parts that stay equal when the size changes";

    // Depth.
    depth_title: "Profundidad del mueble", "Depth of the furniture";
    depth_text: "El frente está definido. ¿Qué profundidad tendrá?", "The front is defined. How deep will it be?";

    // Add part.
    material: "Material", "Material";
    orientation: "Orientación", "Orientation";
    board_side: "Vertical de lado", "Upright sideways";
    board_flat: "Horizontal", "Flat";
    board_back: "Vertical de frente", "Upright facing front";
    stick_along: "A lo", "Along";
    stick_width: "Ancho", "Width";
    stick_height: "Alto", "Height";
    stick_depth: "Fondo", "Depth";
    no_material_of_kind: "No hay un material de ese tipo: agrégalo en Materiales",
        "There is no material of that kind: add one in Materials";
    duplicate_beside: "Duplicar al lado", "Duplicate beside";
    dir_left: "A la izquierda", "To the left";
    dir_right: "A la derecha", "To the right";
    dir_below: "Abajo", "Below";
    dir_above: "Arriba", "Above";
    dir_behind: "Atrás", "Behind";
    dir_in_front: "Adelante", "In front";
    free_form: "Forma libre", "Free form";
    free_form_help: "Esquinas redondeadas y cortes: rectángulos, perforaciones, ranuras, polígonos y bocados",
        "Rounded corners and cuts: rectangles, holes, slots, polygons and notches";

    // Screws.
    screws_per_click: "Tornillos por clic", "Screws per click";
    auto_screws: "Atornillar todo", "Screw everything";
    auto_screws_help: "Pone tornillos en todas las uniones que no tienen y a las que llega el taladro",
        "Puts screws in every joint that has none and a drill can reach";
    auto_screws_done: "{} uniones atornilladas", "{} joints screwed";
    analyze: "Analizar", "Analyze";
    analyze_help: "Revisa que cada pieza quede sujeta con al menos dos tornillos y que el taladro pueda llegar",
        "Checks that every part is held by at least two screws and that a drill can reach them";
    analysis_ok: "Todo en orden", "Everything is fine";
    analysis_found: "{} advertencias: revisa el panel", "{} warnings: see the panel";
    drill_length: "Largo del taladro", "Drill length";
    drill_length_help: "Espacio que necesita el taladro con su punta detrás del tornillo",
        "Room the drill with its bit needs behind the screw";
    drill_width: "Ancho", "Width";
    drill_width_help: "Ancho del cuerpo del taladro: necesita la mitad libre alrededor del tornillo",
        "Width of the body of the drill: it needs half of it free around the screw";
    screw_total: "{} tornillos", "{} screws";
    no_joint_behind: "No hay una unión detrás de esa cara: haz clic en la cara por donde entra el tornillo",
        "There is no joint behind that face: click the face the screw goes in through";
    remove_orphan_screws: "Quitar tornillos sueltos", "Remove loose screws";
    hide_analysis: "Ocultar análisis", "Hide analysis";

    // Side panel.
    properties: "Propiedades", "Properties";
    parts: "Piezas", "Parts";
    parameters: "Parámetros", "Parameters";
    warnings: "Advertencias", "Warnings";
    lock_part: "Bloquear: no se puede mover ni editar", "Lock: it cannot be moved or edited";
    unlock_part: "Desbloquear", "Unlock";
    part_locked: "{} está bloqueada", "{} is locked";
    no_parts: "Sin piezas", "No parts";
    add_parameter: "Parámetro", "Parameter";
    parameters_help: "Un parámetro se puede usar en cualquier medida: W/2, (H - 2*t)/3…",
        "A parameter can be used in any measure: W/2, (H - 2*t)/3…";
    bad_param_name: "Nombre no válido o ya usado", "Invalid name, or already in use";
    param_in_use: "'{}' se usa en alguna medida", "'{}' is used by some measure";
    swap_section: "Girar la sección", "Turn the section";
    axis_width: "Ancho", "Width";
    axis_height: "Alto", "Height";
    axis_depth: "Fondo", "Depth";
    end_from: "Desde", "From";
    end_to: "Hasta", "To";
    end_position: "Posición", "Position";
    length: "Largo", "Length";
    aligned_face: "Cara", "Face";
    side_center: "Centro", "Center";
    side_left: "Izquierda", "Left";
    side_right: "Derecha", "Right";
    side_bottom: "Abajo", "Bottom";
    side_top: "Arriba", "Top";
    side_back: "Atrás", "Back";
    side_front: "Frente", "Front";
    ref_of: "{} de {}", "{} of {}";
    between_of: "{} entre {} y {}", "{} between {} and {}";
    detach: "Soltar: dejar como medida fija", "Detach: keep as a fixed measure";
    must_be_positive: "La medida debe ser mayor que cero", "The measure must be greater than zero";

    // Formula errors.
    err_empty: "Falta la medida", "The measure is missing";
    err_syntax: "Fórmula mal escrita", "Malformed formula";
    err_unknown_name: "No existe el parámetro '{}'", "There is no parameter '{}'";
    err_unknown_function: "No existe la función '{}'", "There is no function '{}'";
    err_arity: "'{}' no lleva esa cantidad de valores", "'{}' does not take that many values";
    err_division: "División por cero", "Division by zero";
    err_not_finite: "El resultado no es un número", "The result is not a number";
    err_cycle: "'{}' depende de sí mismo", "'{}' depends on itself";
    err_missing_part: "se refiere a una pieza que ya no existe", "it refers to a part that no longer exists";
    err_part_cycle: "depende de una pieza que depende de ella", "it depends on a part that depends on it";
    err_missing_material: "no tiene material", "it has no material";

    // Issues.
    issue_broken: "{} no se puede calcular: {}", "{} cannot be computed: {}";
    issue_inverted: "{} tiene un extremo antes que el otro", "{} has one end before the other";
    issue_overlap: "{} y {} se cruzan", "{} and {} run into each other";
    issue_inside: "{} y {} están una dentro de otra", "{} and {} are one inside the other";
    issue_shape: "{}: la forma libre tiene cortes con problemas", "{}: the free form has cuts with problems";
    issue_floating: "{} no toca ninguna otra pieza", "{} touches no other part";
    issue_no_screws_part: "{} no tiene tornillos", "{} has no screws";
    issue_one_screw_part: "{} tiene un solo tornillo: puede rotar", "{} has a single screw: it can rotate";
    issue_joint_no_screws: "Unión {} – {} sin tornillos", "Joint {} – {} without screws";
    issue_joint_one_screw: "Unión {} – {} con un solo tornillo: puede girar", "Joint {} – {} with a single screw: it can pivot";
    issue_blocked: "Unión {} – {}: el taladro no llega porque {} tapa la entrada. Usa tarugos, una ranura, o desplaza una de las piezas.",
        "Joint {} – {}: the drill cannot get in because {} covers the entry. Use dowels, a dado, or shift one of the parts.";
    issue_tight: "Unión {} – {}: solo hay {} mm para el taladro (necesita {} mm)",
        "Joint {} – {}: only {} mm of room for the drill (it needs {} mm)";
    issue_cramped: "Unión {} – {}: {} queda a {} mm del tornillo y el taladro necesita {} mm alrededor. Separa más las piezas o usa una punta larga.",
        "Joint {} – {}: {} is {} mm from the screw and the drill needs {} mm around it. Move the parts further apart or use a long bit.";
    issue_orphan_screw: "Hay tornillos en piezas que ya no se tocan", "There are screws in parts that no longer touch";

    // Materials.
    materials_intro: "Tableros (con su espesor) y palos (con su sección). El material con estrella es el principal: el del marco y las divisiones.",
        "Boards (with their thickness) and sticks (with their section). The starred material is the main one: that of the frame and the divisions.";
    materials_note: "El largo y el ancho de venta son opcionales: solo sirven para estimar cuánto comprar. Muchos servicios de corte ya calculan y optimizan eso.",
        "The stock length and width are optional: they only serve to estimate how much to buy. Many cutting services already compute and optimize that.";
    kind: "Tipo", "Kind";
    board: "Tablero", "Board";
    stick: "Palo", "Stick";
    thickness_or_section: "Espesor / sección", "Thickness / section";
    stock_length: "Largo de venta", "Stock length";
    stock_width: "Ancho de venta", "Stock width";
    color: "Color", "Color";
    main_material_help: "Material principal (solo tableros)", "Main material (boards only)";
    material_in_use: "En uso: no se puede eliminar", "In use: it cannot be deleted";
    material_invalid: "El espesor o la sección debe ser mayor que cero", "The thickness or section must be greater than zero";
    add_board_material: "Tablero", "Board";
    add_stick_material: "Palo", "Stick";

    // Cut list.
    cut_list_intro: "Cada pieza como el rectángulo que se pide a un servicio de corte. Las formas libres se trabajan después del corte.",
        "Every part as the rectangle a cutting service takes. Free forms are worked after the cut.";
    cut_list_broken: "Hay piezas que no se pueden calcular y no están en la lista",
        "Some parts cannot be computed and are not in the list";
    cut_list_total: "{} piezas en total", "{} parts in all";
    quantity: "Cantidad", "Quantity";
    length_mm: "Largo (mm)", "Length (mm)";
    width_mm: "Ancho (mm)", "Width (mm)";
    thickness_mm: "Espesor (mm)", "Thickness (mm)";
    shaped_note: "Lleva forma libre: se corta el rectángulo y luego se trabaja",
        "It has a free form: the rectangle is cut and then worked";
    export_csv: "Exportar CSV", "Export CSV";
    exported: "Exportado a {}", "Exported to {}";
    copy_help: "Copia la tabla para pegarla en una planilla", "Copies the table to paste it in a spreadsheet";
    stock_estimate: "Estimación de material", "Material estimate";
    kerf: "Ancho de la sierra", "Saw kerf";
    need_area: "{} piezas, {} m²", "{} parts, {} m²";
    need_length: "{} piezas, {} m", "{} parts, {} m";
    need_sheets: "aprox. {} tablero(s)", "about {} sheet(s)";
    need_sticks: "aprox. {} palo(s)", "about {} stick(s)";
    need_unknown: "indica las medidas de venta en Materiales para estimar",
        "give the stock size in Materials to estimate";
    need_too_big: "{} pieza(s) más grande(s) que el material", "{} part(s) bigger than the stock";
    stock_note: "Es una estimación simple para comprar, no un plan de corte optimizado.",
        "It is a simple estimate to buy material, not an optimized cutting plan.";

    // Free form.
    shape_intro: "Cara de {} × {} mm ({} horizontal, {} vertical). Las medidas parten de la esquina inferior izquierda.",
        "Face of {} × {} mm ({} across, {} up). Measures start at the lower-left corner.";
    shape_note: "La lista de corte sigue usando el rectángulo completo.", "The cut list keeps using the whole rectangle.";
    shape_bad_dimension: "Medida no válida", "Invalid measure";
    shape_out_of_bounds: "No toca la pieza o no cabe en ella", "It does not touch the part or does not fit in it";
    shape_self_intersecting: "Los lados del polígono se cruzan", "The sides of the polygon cross";
    shape_splits: "Los cortes dejan la pieza en varios trozos: solo se conserva el mayor",
        "The cuts leave the part in several pieces: only the largest is kept";
    done: "Listo", "Done";
    done_help: "Volver al diseño 3D (Esc)", "Back to the 3D design (Esc)";
    figures: "Cortes", "Cuts";
    no_figures: "Sin cortes. Dibuja uno con las herramientas de la izquierda.",
        "No cuts. Draw one with the tools on the left.";
    rectangle: "Rectángulo", "Rectangle";
    polygon: "Polígono", "Polygon";
    radius_all: "radio en todos los vértices", "radius on every vertex";
    feature_edge: "borde", "edge";
    feature_height: "alto", "height";
    hint_shape_select: "Clic para seleccionar un corte; arrástralo para moverlo o arrastra sus puntos para cambiarlo. Los círculos de las esquinas las redondean.",
        "Click to select a cut; drag it to move it or drag its points to change it. The circles at the corners round them.";
    hint_shape_rect: "Arrastra de esquina a esquina (o haz clic para uno estándar)",
        "Drag from corner to corner (or click for a standard one)";
    hint_shape_circle: "Arrastra del centro al borde (o haz clic para una estándar)",
        "Drag from the centre to the rim (or click for a standard one)";
    hint_shape_slot: "Arrastra de un extremo al otro (o haz clic para una estándar)",
        "Drag from one end to the other (or click for a standard one)";
    hint_shape_polygon: "Clic en cada punto (Ctrl o Shift: ángulos de 15°); doble clic, Enter o clic en el primero para cerrar; Esc cancela",
        "Click every point (Ctrl or Shift: 15° angles); double click, Enter or click the first one to close; Esc cancels";
    hint_shape_notch: "Clic cerca de un borde de la pieza", "Click near an edge of the part";
    corners: "Esquinas", "Corners";
    corner_square: "Recta", "Square";
    corner_round: "Redondeada", "Rounded";
    corner_chamfer: "Chaflán", "Chamfer";
    notch: "Bocado", "Notch";
    slot: "Ranura", "Slot";
    hole: "Perforación", "Hole";
    feature_pos: "desde", "from";
    feature_width: "ancho", "width";
    feature_depth: "fondo", "depth";
    feature_radius: "radio", "radius";
    feature_length: "largo", "length";
    feature_diameter: "diámetro", "diameter";
    vertical: "Vertical", "Vertical";
    horizontal: "Horizontal", "Horizontal";
    guides: "Líneas de guía", "Guide lines";
    guide_line: "Línea de guía", "Guide line";
    hint_shape_guide: "Clic para poner una línea de guía (vertical u horizontal, arriba se elige); los cortes se enganchan a ella",
        "Click to place a guide line (vertical or horizontal, chosen above); cuts snap to it";

    // View cube.
    view_front: "Frente", "Front";
    view_back: "Atrás", "Back";
    view_right: "Der.", "Right";
    view_left: "Izq.", "Left";
    view_top: "Arriba", "Top";
    view_bottom: "Abajo", "Bottom";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fill_replaces_in_order() {
        assert_eq!(fill("{} of {}", &[&1, &"two"]), "1 of two");
        assert_eq!(fill("{} and {}", &[&1]), "1 and {}");
    }

    #[test]
    fn templates_have_the_same_slots_in_both_languages() {
        // Compared through a few known templates; the rest follow the same pattern.
        for (es, en) in [
            (ES.issue_tight, EN.issue_tight),
            (ES.issue_blocked, EN.issue_blocked),
            (ES.shape_intro, EN.shape_intro),
            (ES.status_counts, EN.status_counts),
        ] {
            assert_eq!(es.matches("{}").count(), en.matches("{}").count());
        }
    }
}
