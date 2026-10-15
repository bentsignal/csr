// Our compatibility code, not authored game instructions. The movie, fonts,
// panels and bitmap exports are read from the player's installed Legacy HUD.
// Rust owns receipt keys, ordering, lifetime, fade and the bounded notice list.
// Bitmap canvas-to-font sizing is measured against dated 2015 USP-S/headshot
// footage, but its exact native pixels/HUD scaling remain uncalibrated.
_global.csrDeathNotices = new Object();
_global.csrDeathNotices.rows = new Object();
_global.csrDeathNotices.generation = 0;
_global.csrDeathNotices.ready = false;

_global.csrDeathNotices.escape = function(value) {
    return String(value).split("&").join("&amp;").split("<").join("&lt;")
        .split(">").join("&gt;").split('"').join("&quot;")
        .split("'").join("&#39;").split("\r").join(" ").split("\n").join(" ");
};
_global.csrDeathNotices.color = function(value) {
    if (typeof value != "string" || value.length != 7 || value.charAt(0) != "#") { return false; }
    var i = 1;
    while (i < value.length) {
        if ("0123456789abcdefABCDEF".indexOf(value.charAt(i)) < 0) { return false; }
        i += 1;
    }
    return true;
};
_global.csrDeathNotices.key = function(value) {
    if (typeof value != "string" || value.length == 0 || value.length > 128) { return undefined; }
    // Prefix prevents player-like object property names from becoming keys.
    return "receipt:" + value;
};
_global.csrDeathNotices.linkage = function(value) {
    if (typeof value != "string" || value.length > 128 || value.substr(0, 5) != "icon-" ||
        value.substr(value.length - 4) != ".png") { return false; }
    var i = 0;
    var allowed = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_- .";
    while (i < value.length) {
        if (allowed.indexOf(value.charAt(i)) < 0 || value.charAt(i) == " ") { return false; }
        i += 1;
    }
    return true;
};
_global.csrDeathNotices.measure = function(field, html) {
    var previous = field.htmlText;
    field.htmlText = html;
    var width = field.textWidth;
    field.htmlText = previous;
    return width;
};
_global.csrDeathNotices.bitmap = function(parent, name, linkage) {
    var holder = parent.createEmptyMovieClip(name, parent.getNextHighestDepth());
    // Bitmap-export attachment can return undefined in Ruffle even on success.
    // The owned container's actual bounds are the success criterion.
    holder.attachMovie(linkage, "Bitmap", 1);
    var bounds = holder.getBounds(holder);
    var width = bounds.xMax - bounds.xMin;
    var height = bounds.yMax - bounds.yMin;
    if (!(width > 0) || !(height > 0) || width > 4096 || height > 4096) {
        holder.removeMovieClip();
        return undefined;
    }
    return {clip: holder, bounds: bounds, width: width, height: height};
};
_global.csrDeathNotices.fitHorizontal = function(clip, parent, left, right) {
    if (typeof clip != "movieclip" || !(right > left)) { return false; }
    var before = clip.getBounds(parent);
    if (!(before.xMax > before.xMin)) { return false; }
    clip._xscale *= (right - left) / (before.xMax - before.xMin);
    var after = clip.getBounds(parent);
    clip._x += left - after.xMin;
    return true;
};
_global.csrDeathNotices.layout = function(row) {
    var panel = row.panel;
    var parent = panel.TextPanel;
    var field = parent.Text;
    var background = panel.IDTextBG;
    if (field == undefined || background == undefined) { return false; }
    var killer = '<font color="' + row.killerColor + '">' + row.killer + '</font>';
    var victim = '<font color="' + row.victimColor + '">' + row.victim + '</font>';
    // Reset to ordinary populated text on each layout, retaining the original
    // setter's current padding/height instead of accumulating our width delta.
    this.root.SetPanelText(panel, killer + " " + victim, row.localVictim, row.localAttacker);
    // Public observation confirms UpdateWidth refreshes the existing art's
    // text-dependent width. Reset it to this plain text before adding the tab
    // delta, otherwise repeated resize adds that delta to the prior fit again.
    this.root.UpdateWidth(panel);
    var plainWidth = field.textWidth;
    var box = background.getBounds(panel);
    var killerWidth = this.measure(field, killer);
    // A whitespace-only span has zero measured width in this runtime. Measure
    // an actual INTERNAL space using the same original TextField/font instead.
    var space = this.measure(field, "A A") - this.measure(field, "AA");
    if (!(space > 0)) { return false; }

    var top = {x: box.xMin, y: box.yMin};
    var bottom = {x: box.xMin, y: box.yMax};
    panel.localToGlobal(top);
    panel.localToGlobal(bottom);
    parent.globalToLocal(top);
    parent.globalToLocal(bottom);
    var height = Math.abs(bottom.y - top.y);
    if (!(height > 0)) { return false; }
    var fontSize = Number(field.getTextFormat().size);
    if (!(fontSize > 0) || fontSize > 512) { return false; }
    var icons = [row.weapon];
    if (row.head != undefined) { icons.push(row.head); }
    var offsets = [];
    var offset = killerWidth + space;
    var i = 0;
    while (i < icons.length) {
        var icon = icons[i];
        // The original field is 16px and these bitmap canvases are 32px high.
        // In dated 2015 footage, USP-S ink is about 30x11px (canvas ink65x24)
        // and headshot ink is about 12px high (canvas ink26px). Font-height
        // canvas sizing independently agrees for both within footage/HUD-scale
        // uncertainty. Retain the populated row-height bound for other packs.
        var scale = Math.min(1, fontSize / icon.height, height / icon.height);
        icon.clip._xscale = scale * 100;
        icon.clip._yscale = scale * 100;
        offsets.push(offset);
        offset += icon.width * scale + space;
        i += 1;
    }
    // Publicly observed TABSTOPS reservation works in pinned Ruffle. The tab
    // keeps one original field and its original fonts/colors/right alignment.
    var html = '<TEXTFORMAT TABSTOPS="' + String(offset) + '">' + killer + "\t" + victim + '</TEXTFORMAT>';
    this.root.SetPanelText(panel, html, row.localVictim, row.localAttacker);
    var delta = field.textWidth - plainWidth;
    if (!(delta > 0)) { return false; }
    // The installed setter does not include tab advance in its BG measurement.
    // Resize ONLY its existing art by the measured text delta, keeping its
    // original right anchor and padding. Original highlight sprites share the
    // same content rectangle; no replacement rectangle/outline is drawn.
    // Preserve the original anchor across repeated layout/resize calls even if
    // SetPanelText resets width without resetting a prior compatibility x.
    var right = row.backgroundRight;
    var left = right - (box.xMax - box.xMin) - delta;
    if (!this.fitHorizontal(background, panel, left, right) ||
        !this.fitHorizontal(panel.PanelAttacker, panel, left, right) ||
        !this.fitHorizontal(panel.PanelVictim, panel, left, right)) { return false; }

    var format = field.getTextFormat();
    var extent = format.getTextExtent("");
    var gutter = (extent.textFieldWidth - extent.width) / 2;
    if (!(gutter >= 0)) { return false; }
    var bounds = row.fieldBounds;
    var textLeft = bounds.xMin + gutter;
    if (format.align == "right") { textLeft = bounds.xMax - gutter - field.textWidth; }
    else if (format.align == "center") { textLeft = (bounds.xMin + bounds.xMax - field.textWidth) / 2; }
    i = 0;
    while (i < icons.length) {
        var icon = icons[i];
        var scale = icon.clip._xscale / 100;
        icon.clip._x = textLeft + offsets[i] - icon.bounds.xMin * scale;
        icon.clip._y = (top.y + bottom.y) / 2 - (icon.bounds.yMin + icon.height / 2) * scale;
        i += 1;
    }
    panel._visible = true;
    return true;
};

_global.csrDeathNotices.load = function(url) {
    if (typeof url != "string" || url.length == 0 || url.length > 1024) { return false; }
    this.clear();
    this.ready = false;
    this.generation += 1;
    if (this.loader != undefined && this.listener != undefined) {
        this.loader.removeListener(this.listener);
        this.loader.unloadClip(this.root);
    }
    var parent = _global.csrResolve("_level9000");
    if (parent == undefined) { return false; }
    parent._visible = true;
    this.root = parent.createEmptyMovieClip("csrDeathNotice", 200);
    this.root._lockroot = true;
    this.root._visible = true;
    this.loader = new MovieClipLoader();
    var listener = new Object();
    listener.generation = this.generation;
    listener.onLoadInit = function(mc) {
        var api = _global.csrDeathNotices;
        if (api.listener != this || api.generation != this.generation || api.root != mc) { return; }
        this.loader.removeListener(this);
        mc.onLoaded();
        mc.onResize();
        mc._visible = true;
        api.ready = true;
        csrEI.call("csr.DeathNoticesReady", String(mc));
    };
    listener.onLoadError = function(mc, error) {
        var api = _global.csrDeathNotices;
        if (api.listener != this || api.generation != this.generation) { return; }
        this.loader.removeListener(this);
        csrEI.call("csr.DeathNoticesFailed", String(error));
    };
    listener.loader = this.loader;
    this.listener = listener;
    this.loader.addListener(listener);
    return this.loader.loadClip(url, this.root);
};
_global.csrDeathNotices.addImpl = function(key, killer, victim, killerColor, victimColor, weaponLinkage, headshot, localAttacker, localVictim) {
    var id = this.key(key);
    if (!this.ready || id == undefined || this.rows[id] != undefined ||
        typeof killer != "string" || killer.length > 256 || typeof victim != "string" || victim.length > 256 ||
        !this.color(killerColor) || !this.color(victimColor) || !this.linkage(weaponLinkage)) { return false; }
    var panel = this.root.AddPanel(false);
    if (typeof panel != "movieclip") { return false; }
    // Register ownership before any later public movie call can fail. The add
    // wrapper can then hide/remove this partial row without touching siblings.
    var row = {panel: panel};
    this.rows[id] = row;
    if (panel.TextPanel.Text == undefined) { return false; }
    row.killer = this.escape(killer);
    row.victim = this.escape(victim);
    row.killerColor = killerColor;
    row.victimColor = victimColor;
    row.localAttacker = localAttacker === true;
    row.localVictim = localVictim === true;
    // Initialize the actual text-dependent authored row height before fitting
    // icons: the unloaded template height differs from SetPanelText's height.
    this.root.SetPanelText(panel,
        '<font color="' + row.killerColor + '">' + row.killer + '</font> ' +
        '<font color="' + row.victimColor + '">' + row.victim + '</font>',
        row.localVictim, row.localAttacker);
    // Capture before helper children can contribute to MovieClip bounds.
    row.fieldBounds = panel.TextPanel.getBounds(panel.TextPanel);
    row.backgroundRight = panel.IDTextBG.getBounds(panel).xMax;
    row.weapon = this.bitmap(panel.TextPanel, "csrWeapon", weaponLinkage);
    if (headshot === true) { row.head = this.bitmap(panel.TextPanel, "csrHead", "icon-headshot.png"); }
    if (row.weapon == undefined || (headshot === true && row.head == undefined)) {
        return false;
    }
    return this.layout(row);
};
_global.csrDeathNotices.add = function(key, killer, victim, killerColor, victimColor, weaponLinkage, headshot, localAttacker, localVictim) {
    var id = this.key(key);
    var previous = this.rows[id];
    var success = false;
    try {
        success = this.addImpl.apply(this, arguments) === true;
    } catch (error) {
        // Unknown installed/pack content must restore the client's plain feed,
        // including unexpected public movie/layout failures rather than hang.
        success = false;
    }
    // Rejected duplicate keys belong to the existing receipt. Clean only a
    // row newly allocated by this call, including exceptions during layout.
    if (!success && id != undefined && previous == undefined && this.rows[id] != undefined) {
        this.discard(id, this.rows[id]);
    }
    csrEI.call("csr.DeathNoticeResult", String(key), success);
    return success;
};
_global.csrDeathNotices.discard = function(id, row) {
    if (row == undefined || this.rows[id] != row) { return false; }
    row.panel._visible = false;
    // A content-specific RemovePanel failure must leave this owned row hidden
    // and must not prevent its keyed result or another receipt's cleanup.
    try { this.root.RemovePanel(row.panel); }
    catch (error) { }
    delete this.rows[id];
    return true;
};
_global.csrDeathNotices.remove = function(key) {
    var id = this.key(key);
    var row = this.rows[id];
    if (id == undefined || row == undefined) { return false; }
    return this.discard(id, row);
};
_global.csrDeathNotices.alpha = function(key, percent) {
    var id = this.key(key);
    if (id == undefined || this.rows[id] == undefined || typeof percent != "number" ||
        isNaN(percent) || percent < 0 || percent > 100) { return false; }
    this.rows[id].panel._alpha = percent;
    return true;
};
_global.csrDeathNotices.clear = function() {
    var keys = [];
    for (var id in this.rows) {
        // Installed UI helpers add enumerable Object-prototype functions.
        // Only our prefixed rows with an owned panel belong to this list.
        var row = this.rows[id];
        if (id.substr(0, 8) == "receipt:" && typeof row == "object" &&
            typeof row.panel == "movieclip") { keys.push(id.substr(8)); }
    }
    var i = 0;
    while (i < keys.length) { this.remove(keys[i]); i += 1; }
    return true;
};
_global.csrDeathNotices.resize = function() {
    if (!this.ready) { return false; }
    this.root.onResize();
    var success = true;
    for (var id in this.rows) {
        var row = this.rows[id];
        if (id.substr(0, 8) != "receipt:" || typeof row != "object" ||
            typeof row.panel != "movieclip") { continue; }
        var laidOut = false;
        try { laidOut = this.layout(row) === true; }
        catch (error) { laidOut = false; }
        if (!laidOut) {
            success = false;
            row.panel._visible = false;
        }
        // A repaired layout must also clear this key's client-side fallback.
        csrEI.call("csr.DeathNoticeResult", id.substr(8), laidOut);
    }
    return success;
};
