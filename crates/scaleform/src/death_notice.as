// Our compatibility code, not authored game instructions. The movie, fonts,
// panels and bitmap exports are read from the player's installed Legacy HUD.
// Rust owns receipt keys, ordering, lifetime, fade and the bounded notice list.
// Icon row-fit is a measured safety bound, NOT calibrated native image scale.
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
_global.csrDeathNotices.slot = function(field, width, space) {
    // Flash HTML image layout is missing in Ruffle. Reserve its measured width
    // through supported HTML typography so the ORIGINAL SetPanelText still owns
    // background sizing, right alignment, team colors and local-player panels.
    var html = '<font letterSpacing="' + String(width - space) + '">&#160;</font>';
    return {html: html, width: this.measure(field, html)};
};
_global.csrDeathNotices.layout = function(row) {
    var panel = row.panel;
    var parent = panel.TextPanel;
    var field = parent.Text;
    var background = panel.IDTextBG;
    if (field == undefined || background == undefined) { return false; }
    var space = this.measure(field, "&#160;");
    if (!(space > 0)) { return false; }

    // Convert the existing background's row into the TextPanel coordinate
    // system; do not assume a viewport scale or duplicate its padding constants.
    var top = {x: background._x, y: background._y};
    var bottom = {x: background._x, y: background._y + background._height};
    panel.localToGlobal(top);
    panel.localToGlobal(bottom);
    parent.globalToLocal(top);
    parent.globalToLocal(bottom);
    var height = Math.abs(bottom.y - top.y);
    if (!(height > 0)) { return false; }
    var prefix = '<font color="' + row.killerColor + '">' + row.killer + '</font>&#160;';
    var prefixWidth = this.measure(field, prefix);
    var html = prefix;
    var icons = [row.weapon];
    if (row.head != undefined) { icons.push(row.head); }
    var offsets = [];
    var offset = prefixWidth;
    var i = 0;
    while (i < icons.length) {
        var icon = icons[i];
        var scale = Math.min(1, height / icon.height);
        icon.clip._xscale = scale * 100;
        icon.clip._yscale = scale * 100;
        var slot = this.slot(field, icon.width * scale, space);
        if (!(slot.width > 0)) { return false; }
        offsets.push(offset);
        html += slot.html + "&#160;";
        offset += slot.width + space;
        i += 1;
    }
    html += '<font color="' + row.victimColor + '">' + row.victim + '</font>';
    // Public boolean A/B: third enables PanelVictim, fourth PanelAttacker.
    this.root.SetPanelText(panel, html, row.localVictim, row.localAttacker);

    // Bounds include the original field's own placement. Native TextFormat
    // metrics supply its gutter rather than a guessed pixel offset.
    var format = field.getTextFormat();
    var extent = format.getTextExtent("");
    var gutter = (extent.textFieldWidth - extent.width) / 2;
    if (!(gutter >= 0)) { return false; }
    var bounds = parent.getBounds(parent);
    var left = bounds.xMin + gutter;
    if (format.align == "right") { left = bounds.xMax - gutter - field.textWidth; }
    else if (format.align == "center") { left = (bounds.xMin + bounds.xMax - field.textWidth) / 2; }
    i = 0;
    while (i < icons.length) {
        var icon = icons[i];
        var scale = icon.clip._xscale / 100;
        icon.clip._x = left + offsets[i] - icon.bounds.xMin * scale;
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
    if (typeof panel != "movieclip" || panel.TextPanel.Text == undefined) { return false; }
    var row = {panel: panel, killer: this.escape(killer), victim: this.escape(victim),
        killerColor: killerColor, victimColor: victimColor,
        localAttacker: localAttacker === true, localVictim: localVictim === true};
    // Initialize the actual text-dependent authored row height before fitting
    // icons: the unloaded template height differs from SetPanelText's height.
    this.root.SetPanelText(panel,
        '<font color="' + row.killerColor + '">' + row.killer + '</font>&#160;' +
        '<font color="' + row.victimColor + '">' + row.victim + '</font>',
        row.localVictim, row.localAttacker);
    row.weapon = this.bitmap(panel.TextPanel, "csrWeapon", weaponLinkage);
    if (headshot === true) { row.head = this.bitmap(panel.TextPanel, "csrHead", "icon-headshot.png"); }
    if (row.weapon == undefined || (headshot === true && row.head == undefined)) {
        this.root.RemovePanel(panel);
        return false;
    }
    this.rows[id] = row;
    if (!this.layout(row)) { this.remove(key); return false; }
    return true;
};
_global.csrDeathNotices.add = function(key, killer, victim, killerColor, victimColor, weaponLinkage, headshot, localAttacker, localVictim) {
    var success = false;
    try {
        success = this.addImpl.apply(this, arguments) === true;
    } catch (error) {
        // Unknown installed/pack content must restore the client's plain feed,
        // including unexpected public movie/layout failures rather than hang.
        success = false;
    }
    csrEI.call("csr.DeathNoticeResult", String(key), success);
    return success;
};
_global.csrDeathNotices.remove = function(key) {
    var id = this.key(key);
    var row = this.rows[id];
    if (id == undefined || row == undefined) { return false; }
    this.root.RemovePanel(row.panel);
    delete this.rows[id];
    return true;
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
    for (var id in this.rows) { keys.push(id.substr(8)); }
    var i = 0;
    while (i < keys.length) { this.remove(keys[i]); i += 1; }
    return true;
};
_global.csrDeathNotices.resize = function() {
    if (!this.ready) { return false; }
    this.root.onResize();
    var success = true;
    for (var id in this.rows) { if (!this.layout(this.rows[id])) { success = false; } }
    return success;
};
