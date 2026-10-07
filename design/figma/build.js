/*
 * CleanYourMac editable Figma builder.
 *
 * Execute one phase per use_figma call:
 *   return await buildCleanYourMac(figma, { phase: "foundations", tokens, screens, logo });
 *   return await buildCleanYourMac(figma, { phase: "components", tokens, screens, logo });
 *   return await buildCleanYourMac(figma, { phase: "screens", screenName: "Overview", tokens, screens, logo });
 *
 * Input is embedded by prepare-plugin.py. This is a builder, not a flattened
 * screenshot import. All text, charts, rows, controls and vectors stay editable.
 * The helpers follow the Figma skill createVariableCollection,
 * createSemanticTokens, bindVariablesToComponent and component-variant patterns.
 */
async function buildCleanYourMac(figma, input) {
  const created = [];
  const changed = [];
  const { tokens, screens, logo } = input;
  const rgb = (hex) => {
    const h = hex.replace("#", "");
    return { r: parseInt(h.slice(0, 2), 16) / 255, g: parseInt(h.slice(2, 4), 16) / 255, b: parseInt(h.slice(4, 6), 16) / 255 };
  };
  const track = (node) => { created.push(node.id); return node; };
  const available = await figma.listAvailableFontsAsync();
  const fonts = {};
  for (const [name, spec] of Object.entries(tokens.typography)) {
    const found = available.find(({ fontName }) => fontName.family === spec.family && fontName.style.toLowerCase().replace(/\s/g, "") === spec.style.toLowerCase().replace(/\s/g, ""));
    if (!found) throw new Error("Missing required font: " + spec.family + " " + spec.style + ". No font substitution was made.");
    fonts[name] = found.fontName;
  }
  await Promise.all(Object.values(fonts).map((font) => figma.loadFontAsync(font)));
  const pageName = input.phase === "foundations" ? "01 · Foundations" : input.phase === "components" ? "02 · Components" : "03 · App screens";
  let page = figma.root.children.find((node) => node.name === pageName);
  if (!page) { page = track(figma.createPage()); page.name = pageName; }
  await figma.setCurrentPageAsync(page);
  let collections = await figma.variables.getLocalVariableCollectionsAsync();
  let variables = await figma.variables.getLocalVariablesAsync();
  let styles = await figma.getLocalTextStylesAsync();
  const lookup = (name) => variables.find((v) => v.name === name);
  const syntax = (v, swift) => {
    v.setVariableCodeSyntax("iOS", swift);
    v.setVariableCodeSyntax("WEB", "var(--cym-" + v.name.replace(/\//g, "-").replace(/[A-Z]/g, (c) => "-" + c.toLowerCase()) + ")");
  };
  async function collection(name, modes) {
    let c = collections.find((v) => v.name === name);
    if (!c) {
      c = figma.variables.createVariableCollection(name);
      c.renameMode(c.defaultModeId, modes[0]);
      for (const mode of modes.slice(1)) c.addMode(mode);
      collections.push(c); created.push(c.id);
    }
    return c;
  }
  function variable(name, c, type, values, scopes, swift) {
    let v = variables.find((n) => n.name === name && n.variableCollectionId === c.id);
    if (!v) { v = figma.variables.createVariable(name, c, type); variables.push(v); created.push(v.id); }
    v.scopes = scopes;
    syntax(v, swift);
    for (const mode of c.modes) if (values[mode.name] !== undefined) v.setValueForMode(mode.modeId, values[mode.name]);
    return v;
  }
  function paint(name) {
    const v = lookup("color/" + name);
    if (!v) throw new Error("Run the foundations phase before components/screens: missing color/" + name);
    return figma.variables.setBoundVariableForPaint({ type: "SOLID", color: rgb(tokens.colors[name].light) }, "color", v);
  }
  function float(node, field, family, name) {
    const v = lookup(family + "/" + name);
    if (!v) throw new Error("Missing token: " + family + "/" + name);
    node.setBoundVariable(field, v);
  }
  function frame(name, direction, parent, width, fill, pad = "lg", gap = "md", radius) {
    const n = track(figma.createAutoLayout(direction));
    n.name = name; n.fills = fill ? [paint(fill)] : [];
    n.primaryAxisAlignItems = "MIN"; n.counterAxisAlignItems = "MIN";
    n.primaryAxisSizingMode = "AUTO"; n.counterAxisSizingMode = "FIXED";
    n.resize(width, 1);
    parent.appendChild(n);
    for (const key of ["paddingLeft", "paddingRight", "paddingTop", "paddingBottom"]) float(n, key, "space", pad);
    float(n, "itemSpacing", "space", gap);
    if (radius) float(n, "cornerRadius", "radius", radius);
    return n;
  }
  async function text(value, role, parent, width, color = "ink", name) {
    const n = track(figma.createText());
    n.name = name || role; n.fontName = fonts[role]; n.characters = value;
    const s = styles.find((v) => v.name === "Comfy/" + role);
    if (!s) throw new Error("Missing text style " + role);
    await n.setTextStyleIdAsync(s.id);
    n.fills = [paint(color)]; n.textAutoResize = "HEIGHT";
    n.resize(width, n.height || 20); parent.appendChild(n);
    return n;
  }
  function fillWidth(n) { n.layoutSizingHorizontal = "FILL"; return n; }
  function exposed(c, node, name, value) {
    const key = c.addComponentProperty(name, "TEXT", value);
    node.componentPropertyReferences = { characters: key };
    return key;
  }
  function component(name, width, height, fill = "surface", radius = "control") {
    const n = track(figma.createComponent());
    n.name = name; n.layoutMode = "HORIZONTAL";
    n.resize(width, height); n.fills = [paint(fill)];
    n.primaryAxisAlignItems = "MIN"; n.counterAxisAlignItems = "CENTER";
    float(n, "cornerRadius", "radius", radius);
    for (const key of ["paddingLeft", "paddingRight"]) float(n, key, "space", "lg");
    for (const key of ["paddingTop", "paddingBottom"]) float(n, key, "space", "sm");
    float(n, "itemSpacing", "space", "md");
    return n;
  }
  function combine(name, comps, x, y, columns) {
    const set = track(figma.combineAsVariants(comps, page)); set.name = name;
    set.description = name + " — CleanYourMac shared vocabulary. Use instances. Keyboard and accessibility behavior come from the SwiftUI component.";
    const canonicalProperties = {};
    for (const [key, definition] of Object.entries(set.componentPropertyDefinitions)) {
      if (definition.type === "TEXT") canonicalProperties[key.split("#")[0]] ||= key;
    }
    for (const c of comps) for (const t of c.findAllWithCriteria({ types: ["TEXT"] })) {
      const key = t.componentPropertyReferences && t.componentPropertyReferences.characters;
      if (key && canonicalProperties[key.split("#")[0]]) t.componentPropertyReferences = { characters: canonicalProperties[key.split("#")[0]] };
    }
    for (const [key, definition] of Object.entries(set.componentPropertyDefinitions)) {
      if (definition.type === "TEXT" && canonicalProperties[key.split("#")[0]] !== key) set.deleteComponentProperty(key);
    }
    set.x = x; set.y = y;
    const width = Math.max(...comps.map((c) => c.width));
    const height = Math.max(...comps.map((c) => c.height));
    comps.forEach((c, i) => { c.x = 24 + i % columns * (width + 24); c.y = 24 + Math.floor(i / columns) * (height + 24); });
    set.resize(columns * (width + 24) + 24, Math.ceil(comps.length / columns) * (height + 24) + 24);
    return set;
  }
  function inst(c, parent, values = {}, width) {
    const n = track(c.createInstance()); parent.appendChild(n);
    const definitions = c.parent && c.parent.type === "COMPONENT_SET" ? c.parent.componentPropertyDefinitions : c.componentPropertyDefinitions;
    const props = {};
    for (const [name, value] of Object.entries(values)) {
      const key = Object.keys(definitions).find((key) => key.split("#")[0] === name);
      if (!key) throw new Error(c.name + " lacks text property " + name);
      props[key] = value;
    }
    n.setProperties(props);
    if (width) n.resize(width, n.height);
    return n;
  }
  async function findComponent(name, variant = {}) {
    const cp = figma.root.children.find((p) => p.name === "02 · Components");
    if (!cp) throw new Error("Run the components phase first.");
    // Nodes on another page are accessed by stored IDs in the local plugin run.
    // The tool version supplies a component ledger to avoid loading more pages.
    let node = input.componentIDs && input.componentIDs[name] ? await figma.getNodeByIdAsync(input.componentIDs[name]) : null;
    if (!node && cp.id === page.id) node = cp.children.find((n) => n.name === name);
    if (!node) throw new Error("Pass componentIDs from the component phase. Missing " + name);
    if (node.type === "COMPONENT") return node;
    if (node.type !== "COMPONENT_SET") throw new Error(name + " is not a component.");
    return node.children.find((c) => c.type === "COMPONENT" && Object.entries(variant).every(([key, value]) => c.variantProperties[key] === value)) || node.defaultVariant;
  }

  if (input.phase === "foundations") {
    const primitives = await collection("Comfy · Primitives", ["Value"]);
    const colors = await collection("Comfy · Colors", ["Light", "Dark"]);
    const dimension = await collection("Comfy · Dimensions", ["Value"]);
    for (const [name, modes] of Object.entries(tokens.colors)) {
      const light = variable("primitive/" + name + "/light", primitives, "COLOR", { Value: rgb(modes.light) }, [], "ComfyPrimitive." + name + "Light");
      const dark = variable("primitive/" + name + "/dark", primitives, "COLOR", { Value: rgb(modes.dark) }, [], "ComfyPrimitive." + name + "Dark");
      variable("color/" + name, colors, "COLOR", { Light: { type: "VARIABLE_ALIAS", id: light.id }, Dark: { type: "VARIABLE_ALIAS", id: dark.id } }, ["FRAME_FILL", "SHAPE_FILL", "TEXT_FILL", "STROKE_COLOR"], "Palette." + name);
    }
    for (const [name, value] of Object.entries(tokens.spacing)) variable("space/" + name, dimension, "FLOAT", { Value: value }, ["GAP"], "Space." + name);
    for (const [name, value] of Object.entries(tokens.radius)) variable("radius/" + name, dimension, "FLOAT", { Value: value }, ["CORNER_RADIUS"], "Layout." + name + "Radius");
    for (const [name, value] of Object.entries(tokens.layout)) variable("layout/" + name, dimension, "FLOAT", { Value: value }, ["WIDTH_HEIGHT"], "Layout." + name);
    for (const [role, spec] of Object.entries(tokens.typography)) {
      let s = styles.find((v) => v.name === "Comfy/" + role);
      if (!s) { s = figma.createTextStyle(); s.name = "Comfy/" + role; styles.push(s); created.push(s.id); }
      s.fontName = fonts[role]; s.fontSize = spec.size; s.lineHeight = { unit: "PIXELS", value: spec.lineHeight };
      s.description = "SwiftUI TypeStyle." + role;
    }
    let board = page.children.find((n) => n.name === "Comfy · Foundations");
    if (!board) {
      board = frame("Comfy · Foundations", "VERTICAL", page, 1120, "canvas", "xxl", "xl", "window");
      board.x = 200; board.y = 100;
      await text("Comfy by CleanYourMac", "pageTitle", board, 980);
      await text("A joyful, restful visual language for a considered cleanup.", "secondary", board, 980, "muted");
      const mark = track(figma.createNodeFromSvg(logo)); mark.name = "Brand/Leaf sparkle"; mark.resize(120, 120); board.appendChild(mark);
      const swatches = frame("Semantic colors", "HORIZONTAL", board, 1056, undefined, "xxs", "lg");
      for (const name of ["canvas", "sidebar", "mintSoft", "accent", "coral", "lavender", "amber"]) {
        const swatch = frame("Swatch/" + name, "VERTICAL", swatches, 128, name, "md", "sm", "control");
        await text(name, "caption", swatch, 104, name === "accent" ? "onAccent" : "ink");
        await text(tokens.colors[name].light, "code", swatch, 104, name === "accent" ? "onAccent" : "ink");
      }
      for (const role of ["pageTitle", "sectionTitle", "body", "secondary", "caption", "metric", "code"]) await text(role + "  ·  A little room to breathe.  76.8 GB", role, board, 980);
      await text("Space 2 / 4 / 8 / 12 / 16 / 24 / 32 / 48     Cards 20     Controls 12     Pills 999", "secondary", board, 980, "muted");
      await text("Light and Dark are modes of the same semantic collection. Use text labels beside all category colors.", "secondary", board, 980, "muted");
    }
    return { createdNodeIds: created, mutatedNodeIds: changed, pageID: page.id, boardID: board.id, variables: variables.length, styles: styles.map((s) => ({ id: s.id, name: s.name })), collections: collections.map((c) => ({ id: c.id, name: c.name, modes: c.modes })) };
  }

  if (input.phase === "components") {
    const ids = {};
    for (const name of ["ActionButton", "SidebarItem", "PathChip", "StatusBadge", "ResultRow", "SearchField", "MetricTile", "KeyValueRow", "EmptyState"]) {
      const existing = page.children.find((n) => n.name === name);
      if (existing) ids[name] = existing.id;
    }
    if (!ids.ActionButton) {
      const variants = [];
      for (const kind of ["Primary", "Secondary", "Destructive"]) for (const state of ["Default", "Hover", "Disabled", "Busy"]) {
        const fill = kind === "Primary" ? "accent" : kind === "Destructive" ? "coralSoft" : "surface";
        const c = component("Kind=" + kind + ", State=" + state, 164, 40, fill);
        if (state === "Disabled") c.opacity = 0.45;
        if (state === "Hover") c.strokes = [paint("accent")];
        const label = await text(state === "Busy" ? "Working…" : "Review selection", "body", c, 132, kind === "Primary" ? "onAccent" : kind === "Destructive" ? "destructive" : "ink", "Label");
        exposed(c, label, "Label", label.characters); variants.push(c);
      }
      ids.ActionButton = combine("ActionButton", variants, 200, 100, 4).id;
    }
    if (!ids.SidebarItem) {
      const variants = [];
      for (const state of ["Default", "Selected"]) {
        const c = component("State=" + state, 188, 36, state === "Selected" ? "mintSoft" : "sidebar");
        const label = await text("Node dependencies", "body", c, 156, "ink", "Label"); exposed(c, label, "Label", label.characters); variants.push(c);
      }
      ids.SidebarItem = combine("SidebarItem", variants, 200, 480, 2).id;
    }
    if (!ids.PathChip) {
      const variants = [];
      for (const kind of ["Included", "Excluded"]) {
        const c = component("Kind=" + kind, 256, 32, kind === "Included" ? "mintSoft" : "coralSoft", "pill");
        const label = await text("~/projects", "code", c, 224, "ink", "Path"); exposed(c, label, "Path", "~/projects"); variants.push(c);
      }
      ids.PathChip = combine("PathChip", variants, 200, 640, 2).id;
    }
    if (!ids.StatusBadge) {
      const variants = [];
      for (const tone of ["mint", "coral", "lavender", "amber"]) {
        const c = component("Tone=" + tone, 152, 28, tone + "Soft", "pill");
        const label = await text(tone === "coral" ? "Orphaned" : "Rebuildable", "caption", c, 120, tone === "coral" ? "destructive" : "ink", "Label");
        exposed(c, label, "Label", label.characters); variants.push(c);
      }
      ids.StatusBadge = combine("StatusBadge", variants, 200, 800, 4).id;
    }
    if (!ids.ResultRow) {
      const variants = [];
      for (const state of ["Default", "Focused", "Checked", "Blocked"]) {
        const c = component("State=" + state, 600, 72, state === "Focused" ? "selection" : "surface");
        const checkbox = track(figma.createRectangle()); checkbox.name = "Review checkbox"; checkbox.resize(16, 16); checkbox.cornerRadius = 4; checkbox.fills = state === "Checked" ? [paint("accent")] : []; checkbox.strokes = [paint("border")]; c.appendChild(checkbox);
        const title = frame("Name and path", "VERTICAL", c, 336, undefined, "xxs", "xxs");
        const t = await text("atlas-web · node_modules", "body", title, 332, "ink", "Title");
        const p = await text("~/projects/atlas-web/node_modules", "code", title, 332, "muted", "Path");
        exposed(c, t, "Title", t.characters); exposed(c, p, "Path", p.characters);
        const right = frame("Size and status", "VERTICAL", c, 160, undefined, "xxs", "xxs"); right.counterAxisAlignItems = "MAX";
        const v = await text("12.8 GB", "body", right, 156, "ink", "Value"); v.textAlignHorizontal = "RIGHT";
        const b = await text(state === "Blocked" ? "Protected" : "Rebuildable", "caption", right, 156, state === "Blocked" ? "warning" : "muted", "Badge"); b.textAlignHorizontal = "RIGHT";
        exposed(c, v, "Value", v.characters); exposed(c, b, "Badge", b.characters);
        variants.push(c);
      }
      ids.ResultRow = combine("ResultRow", variants, 200, 980, 2).id;
    }
    if (!ids.SearchField) {
      const variants = [];
      for (const state of ["Empty", "Filled"]) {
        const c = component("State=" + state, 480, 44);
        c.strokes = [paint("border")];
        const t = await text(state === "Empty" ? "Search by name" : "atlas", "body", c, 448, state === "Empty" ? "muted" : "ink", "Query"); exposed(c, t, "Query", t.characters); variants.push(c);
      }
      ids.SearchField = combine("SearchField", variants, 200, 1240, 2).id;
    }
    if (!ids.MetricTile) {
      const c = component("MetricTile", 208, 112, "surface", "card"); c.layoutMode = "VERTICAL"; c.counterAxisAlignItems = "MIN";
      const label = await text("Matched size", "caption", c, 176, "muted", "Label");
      const value = await text("23.8 GB", "metric", c, 176, "ink", "Value");
      const detail = await text("Nested folders counted once", "caption", c, 176, "muted", "Detail");
      exposed(c, label, "Label", label.characters); exposed(c, value, "Value", value.characters); exposed(c, detail, "Detail", detail.characters);
      c.x = 200; c.y = 1400; ids.MetricTile = c.id;
    }
    if (!ids.KeyValueRow) {
      const c = component("KeyValueRow", 224, 52); c.layoutMode = "VERTICAL"; c.counterAxisAlignItems = "MIN";
      const label = await text("Package manager", "caption", c, 192, "muted", "Label");
      const value = await text("pnpm · lockfile found", "body", c, 192, "ink", "Value");
      exposed(c, label, "Label", label.characters); exposed(c, value, "Value", value.characters);
      c.x = 460; c.y = 1400; ids.KeyValueRow = c.id;
    }
    if (!ids.EmptyState) {
      const c = component("EmptyState", 440, 180, "mintSoft", "card"); c.layoutMode = "VERTICAL"; c.counterAxisAlignItems = "CENTER";
      const title = await text("Room to breathe.", "sectionTitle", c, 408); title.textAlignHorizontal = "CENTER";
      const message = await text("No matching items in these folders. Try another name or update the search scope.", "secondary", c, 408, "muted"); message.textAlignHorizontal = "CENTER";
      exposed(c, title, "Title", title.characters); exposed(c, message, "Message", message.characters);
      c.x = 740; c.y = 1400; ids.EmptyState = c.id;
    }
    return { createdNodeIds: created, mutatedNodeIds: changed, pageID: page.id, componentIDs: ids, familyCount: Object.keys(ids).length };
  }

  const data = screens.screens.find((s) => s.name === input.screenName);
  if (!data) throw new Error("Unknown screen " + input.screenName);
  const old = page.children.find((n) => n.name === "Screen/" + data.name);
  if (old && input.phase !== "audit") return { createdNodeIds: [], mutatedNodeIds: [], screenID: old.id, skippedExisting: true, reason: "Inspect existing frame before authorizing an update." };
  if (input.phase === "audit") {
    if (!old) throw new Error("Screen not found");
    const nodes = old.findAll(() => true);
    const nodeTypes = {};
    for (const n of nodes) nodeTypes[n.type] = (nodeTypes[n.type] || 0) + 1;
    const images = nodes.filter((n) => "fills" in n && Array.isArray(n.fills) && n.fills.some((f) => f.type === "IMAGE"));
    const unexpectedFonts = nodes.filter((n) => n.type === "TEXT").filter((n) => !n.getStyledTextSegments(["fontName"]).every((s) => Object.values(fonts).some((f) => f.family === s.fontName.family)));
    return { screenID: old.id, descendantCount: nodes.length, nodeTypes, imageNodes: images.map((n) => ({ id: n.id, name: n.name, width: n.width, height: n.height })), unexpectedFonts: unexpectedFonts.map((n) => n.id), bounds: { width: old.width, height: old.height } };
  }
  const [button, nav, include, exclude, row, search, metric, keyValue, badge] = await Promise.all([
    findComponent("ActionButton", { Kind: "Primary", State: "Default" }), findComponent("SidebarItem", { State: "Default" }),
    findComponent("PathChip", { Kind: "Included" }), findComponent("PathChip", { Kind: "Excluded" }),
    findComponent("ResultRow"), findComponent("SearchField", { State: data.query ? "Filled" : "Empty" }),
    findComponent("MetricTile"), findComponent("KeyValueRow"), findComponent("StatusBadge")
  ]);
  const root = frame("Screen/" + data.name, "VERTICAL", page, 1200, "canvas", "xxs", "xxs", "window");
  root.x = 200 + screens.screens.findIndex((s) => s.name === data.name) % 3 * 1320;
  root.y = 100 + Math.floor(screens.screens.findIndex((s) => s.name === data.name) / 3) * 980;
  root.resize(1200, 820); root.primaryAxisSizingMode = "FIXED"; root.clipsContent = true;
  const titlebar = frame("Window titlebar", "HORIZONTAL", root, 1196, "canvas", "lg", "sm");
  for (const color of ["coral", "amber", "mint"]) { const dot = track(figma.createEllipse()); dot.resize(12, 12); dot.fills = [paint(color)]; titlebar.appendChild(dot); }
  await text("CleanYourMac", "caption", titlebar, 1000, "muted");
  const body = frame("Workspace", "HORIZONTAL", root, 1196, undefined, "xxs", "xxs");
  body.layoutSizingVertical = "FILL"; body.primaryAxisSizingMode = "FIXED";
  const sidebar = frame("Sidebar", "VERTICAL", body, 220, "sidebar", "lg", "xs");
  sidebar.layoutSizingVertical = "FILL"; sidebar.primaryAxisSizingMode = "FIXED";
  const mark = track(figma.createNodeFromSvg(logo)); mark.resize(52, 52); mark.name = "Brand/Leaf sparkle"; sidebar.appendChild(mark);
  await text("CleanYourMac", "sectionTitle", sidebar, 188);
  await text("A little room to breathe.", "caption", sidebar, 188, "muted");
  for (const item of screens.navigation) {
    const active = item.name === data.name;
    const c = active ? await findComponent("SidebarItem", { State: "Selected" }) : nav;
    inst(c, sidebar, { Label: item.name }, 188);
  }
  inst(nav, sidebar, { Label: "Settings" }, 188);
  const contentWidth = data.kind === "finder" ? 660 : 972;
  const main = frame("Main content", "VERTICAL", body, contentWidth, undefined, "xl", "lg");
  main.layoutSizingVertical = "FILL"; main.primaryAxisSizingMode = "FIXED";
  await text(data.title, "pageTitle", main, contentWidth - 48);
  await text(data.subtitle, "secondary", main, contentWidth - 48, "muted");
  if (data.kind === "finder") {
    const scope = frame("Independent search scope", "VERTICAL", main, contentWidth - 48, undefined, "xxs", "xs");
    const included = frame("Included paths", "HORIZONTAL", scope, contentWidth - 52, undefined, "xxs", "sm");
    await text("Include", "caption", included, 48, "muted");
    for (const path of data.included) inst(include, included, { Path: path }, data.included.length > 1 ? 242 : 518);
    const excluded = frame("Excluded paths", "HORIZONTAL", scope, contentWidth - 52, undefined, "xxs", "sm");
    await text("Exclude", "caption", excluded, 48, "muted");
    for (const path of data.excluded.slice(0, 2)) inst(exclude, excluded, { Path: path }, data.excluded.length > 1 ? 242 : 518);
    const searchline = frame("Search and sort", "HORIZONTAL", main, contentWidth - 48, undefined, "xxs", "sm");
    inst(search, searchline, { Query: data.query || "Search by name" }, 376);
    inst(button, searchline, { Label: data.sort }, 224);
    const totals = frame("Aggregated matching results — directly below search", "HORIZONTAL", main, contentWidth - 48, undefined, "xxs", "sm");
    for (const m of data.summary) inst(metric, totals, { Label: m.label, Value: m.value, Detail: m.detail }, 198);
    const table = frame("Results sorted by current control", "VERTICAL", main, contentWidth - 48, "surface", "xs", "xxs", "card");
    table.layoutSizingVertical = "FILL"; table.primaryAxisSizingMode = "FIXED";
    table.clipsContent = true; table.overflowDirection = "VERTICAL_SCROLLING";
    for (const r of data.rows) {
      const c = await findComponent("ResultRow", { State: r.blocked ? "Blocked" : r.checked ? "Checked" : "Default" });
      inst(c, table, { Title: r.title, Path: r.path, Value: r.value, Badge: r.badge }, contentWidth - 56);
    }
    await text(data.note, "caption", main, contentWidth - 48, "muted");
    const footer = frame("Review selection footer", "HORIZONTAL", main, contentWidth - 48, "mintSoft", "lg", "md", "card");
    await text(data.selection, "body", footer, 344);
    inst(button, footer, { Label: data.action }, 204);
    const inspector = frame("Inspector", "VERTICAL", body, 312, "surface", "xl", "lg");
    inspector.layoutSizingVertical = "FILL"; inspector.primaryAxisSizingMode = "FIXED";
    await text(data.inspector.title, "sectionTitle", inspector, 264);
    inst(badge, inspector, { Label: data.inspector.badge }, 180);
    for (const [label, value] of data.inspector.facts) inst(keyValue, inspector, { Label: label, Value: value }, 264);
    await text(data.inspector.consequence, "secondary", inspector, 264, "muted");
    inst(button, inspector, { Label: "Reveal in Finder" }, 264);
  } else if (data.kind === "overview") {
    const header = frame("Overview controls", "HORIZONTAL", main, 924, undefined, "xxs", "md");
    inst(button, header, { Label: "Scan enabled modules" }, 208);
    await text("Last scanned just now · fixture data", "caption", header, 660, "muted");
    const totals = frame("Aggregated analytics", "HORIZONTAL", main, 924, undefined, "xxs", "lg");
    for (const m of data.metrics) inst(metric, totals, { Label: m.label, Value: m.value, Detail: m.detail }, 298);
    const charts = frame("Storage analytics", "HORIZONTAL", main, 924, undefined, "xxs", "lg");
    const ringCard = frame("Disk usage chart", "VERTICAL", charts, 350, "surface", "xl", "lg", "card");
    await text("Your Mac's breathing room", "sectionTitle", ringCard, 302);
    const chart = track(figma.createNodeFromSvg('<svg xmlns="http://www.w3.org/2000/svg" width="240" height="240" viewBox="0 0 240 240"><circle cx="120" cy="120" r="88" fill="none" stroke="' + tokens.colors.mintSoft.light + '" stroke-width="26"/><path d="M120 32A88 88 0 1 1 43.5 163.5" fill="none" stroke="' + tokens.colors.accent.light + '" stroke-width="26" stroke-linecap="round"/></svg>'));
    chart.name = "Editable disk usage vector"; ringCard.appendChild(chart);
    await text("168 GB free", "metric", ringCard, 302);
    await text("332 GB used of 500 GB", "caption", ringCard, 302, "muted");
    const categories = frame("Largest finding categories", "VERTICAL", charts, 558, "surface", "xl", "lg", "card");
    await text("Where the space goes", "sectionTitle", categories, 510);
    for (const cat of data.categories) {
      const label = frame(cat.name, "HORIZONTAL", categories, 510, undefined, "xxs", "sm");
      await text(cat.name, "body", label, 390); await text(cat.value, "body", label, 100);
      const bar = frame("Bar/" + cat.name, "HORIZONTAL", categories, 510, "canvas", "xxs", "xxs", "pill");
      bar.resize(510, 12); bar.primaryAxisSizingMode = "FIXED";
      const segment = track(figma.createRectangle()); segment.resize(Math.max(8, 510 * cat.fraction / 0.31), 8); segment.cornerRadius = 4; segment.fills = [paint(cat.tone)]; bar.appendChild(segment);
    }
    const note = frame("Next step", "VERTICAL", main, 924, "mintSoft", "xl", "sm", "card");
    await text("Start with things you can rebuild.", "sectionTitle", note, 876);
    await text("Node dependencies and generated build data often make good first reviews. Every action shows its paths and consequences before it runs.", "secondary", note, 876, "muted");
  } else if (data.kind === "review") {
    const review = frame("Action review", "VERTICAL", main, 780, "surface", "xl", "xl", "card");
    const total = frame("Review total", "HORIZONTAL", review, 732, "mintSoft", "lg", "md", "card");
    await text("2 folders · " + data.total, "metric", total, 700);
    for (const r of data.rows) inst(row, review, { Title: r.title, Path: r.path, Value: r.value, Badge: r.badge }, 732);
    await text(data.consequence, "secondary", review, 732);
    await text("Paths and folder contents are verified again before the move.", "caption", review, 732, "muted");
    const controls = frame("Action controls", "HORIZONTAL", review, 732, undefined, "xxs", "md");
    const secondary = await findComponent("ActionButton", { Kind: "Secondary", State: "Default" });
    inst(secondary, controls, { Label: "Cancel" }, 164); inst(button, controls, { Label: data.action }, 224);
  } else if (data.kind === "settings") {
    for (const section of data.sections) {
      const card = frame(section.title, "VERTICAL", main, 924, "surface", "xl", "lg", "card");
      await text(section.title, "sectionTitle", card, 876);
      for (const [label, value] of section.rows) {
        const setting = frame(label, "HORIZONTAL", card, 876, undefined, "xxs", "lg");
        await text(label, "body", setting, 264); await text(value, "secondary", setting, 596, "muted");
      }
    }
  }
  return { createdNodeIds: created, mutatedNodeIds: changed, screenID: root.id, pageID: page.id, name: root.name, bounds: { width: root.width, height: root.height }, componentInstances: root.findAllWithCriteria({ types: ["INSTANCE"] }).length };
}
