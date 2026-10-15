// Our owned lifecycle adapter. Original movies, public methods and presentation
// come from the player's installed Legacy data; no authored instructions here.
_global.csrCombatHud = new Object();
_global.csrCombatHud.generation = 0;
_global.csrCombatHud.parts = [];
_global.csrCombatHud.part = function(name) {
    var i = 0;
    while (i < this.parts.length) {
        if (this.parts[i].name == name) { return this.parts[i]; }
        i += 1;
    }
    return undefined;
};
_global.csrCombatHud.failed = function(part, reason) {
    if (part == undefined) { return; }
    part.ready = false;
    part.root._visible = false;
    csrEI.call("csr.CombatHudFailed", part.name, String(reason));
};
_global.csrCombatHud.loadPart = function(name, url, depth) {
    var parent = _global.csrResolve("_level9000");
    if (parent == undefined) { return false; }
    parent._visible = true;
    var part = {name: name, ready: false, generation: this.generation};
    part.root = parent.createEmptyMovieClip("csrCombat" + name, depth);
    part.root._lockroot = true;
    part.root._visible = false;
    part.loader = new MovieClipLoader();
    var listener = {part: part};
    listener.onLoadInit = function(mc) {
        var api = _global.csrCombatHud;
        var part = this.part;
        if (part.generation != api.generation || api.part(part.name) != part ||
            part.listener != this || part.root != mc) { return; }
        part.loader.removeListener(this);
        try {
            if (typeof mc.onLoaded != "function" || typeof mc.onResize != "function" ||
                typeof mc.showNow != "function" || typeof mc.hideNow != "function" ||
                (part.name == "health" && typeof mc.updateValues != "function") ||
                (part.name == "ammo" && typeof mc.updateAmmo != "function")) {
                api.failed(part, "Unsupported public movie interface");
                return;
            }
            mc.onLoaded();
            mc.onResize();
            // Authored sample values must never become a gameplay raster.
            mc._visible = false;
            part.ready = true;
            csrEI.call("csr.CombatHudReady", part.name);
        } catch (error) {
            api.failed(part, "Initialization failed");
        }
    };
    listener.onLoadError = function(mc, error) {
        var api = _global.csrCombatHud;
        var part = this.part;
        if (part.generation != api.generation || api.part(part.name) != part ||
            part.listener != this || part.root != mc) { return; }
        part.loader.removeListener(this);
        api.failed(part, error);
    };
    part.listener = listener;
    this.parts.push(part);
    part.loader.addListener(listener);
    if (!part.loader.loadClip(url, part.root)) {
        part.loader.removeListener(listener);
        this.failed(part, "Could not start movie load");
        return false;
    }
    return true;
};
_global.csrCombatHud.load = function() {
    this.generation += 1;
    var i = 0;
    while (i < this.parts.length) {
        var old = this.parts[i];
        old.ready = false;
        old.root._visible = false;
        old.loader.removeListener(old.listener);
        old.loader.unloadClip(old.root);
        old.root.removeMovieClip();
        i += 1;
    }
    this.parts = [];
    this.loadPart("health", "hudhealtharmormodule.swf", 201);
    this.loadPart("ammo", "hudweaponmodule.swf", 202);
};
_global.csrCombatHud.clear = function() {
    var i = 0;
    while (i < this.parts.length) {
        var part = this.parts[i];
        // Keep one parsed instance per component across matches. Visibility is
        // owned explicitly; parking the shared worker suspends all its timers.
        part.root._visible = false;
        part.shown = false;
        i += 1;
    }
};
_global.csrCombatHud.write = function(field, value) {
    if (field == undefined || typeof field.getTextFormat != "function" ||
        typeof field.setTextFormat != "function" || typeof field.setNewTextFormat != "function") {
        return false;
    }
    // TextField.text resets spans to its default format in Ruffle. Preserve
    // the original template's current font, alignment, colour and decoration,
    // rather than substituting a manually constructed format or new font.
    var format = field.getTextFormat();
    field.text = String(value);
    field.setTextFormat(format);
    field.setNewTextFormat(format);
    return field.text == String(value);
};
_global.csrCombatHud.updateNumbers = function(health, armor, clip, reserve) {
    var hp = this.part("health");
    var ammo = this.part("ammo");
    var healthSuccess = false;
    var ammoSuccess = false;
    if (hp != undefined && hp.ready) {
        try {
            var panel = hp.root.HudPanel.HealthArmorPanel;
            healthSuccess = this.write(panel.Health.TextBox, health) &&
                this.write(panel.HealthRed.TextBox, health) &&
                this.write(panel.Armor.TextBox, armor);
            hp.root._visible = healthSuccess;
            if (healthSuccess) { hp.root.showNow(); }
        } catch (error) { healthSuccess = false; hp.root._visible = false; }
    }
    if (ammo != undefined && ammo.ready) {
        try {
            var panel = ammo.root.HudPanel.WeaponPanel;
            ammoSuccess = this.write(panel.AmmoCountClip.TextBox, clip) &&
                this.write(panel.AmmoCountTotal.TextBox, "/" + reserve);
            ammo.root._visible = ammoSuccess;
            if (ammoSuccess) { ammo.root.showNow(); }
        } catch (error) { ammoSuccess = false; ammo.root._visible = false; }
    }
    csrEI.call("csr.CombatHudNumbers", healthSuccess, ammoSuccess);
    return healthSuccess && ammoSuccess;
};
_global.csrCombatHud.result = function(part, stamp, success, visible) {
    if (part == undefined) { return false; }
    try {
        if (success && visible && !part.shown) {
            part.root.showNow();
            part.shown = true;
        }
        part.root._visible = success && visible;
        if (success && visible) {
            // Independently cropped rasters must contain the complete visible
            // intersection of their original module. Expected offscreen art
            // is clipped by the original viewport, never stretched into ours.
            var bounds = part.root.getBounds(_global.csrResolve("_level9000"));
            var left = Math.max(0, bounds.xMin);
            var right = Math.min(Stage.width, bounds.xMax);
            var top = Math.max(0, bounds.yMin);
            var bottom = Math.min(Stage.height, bounds.yMax);
            if (!(right > left) || !(bottom > top) || top < Math.ceil(Stage.height / 2) ||
                (part.name == "health" && right > Math.ceil(Stage.width / 2)) ||
                (part.name == "ammo" && left < Math.ceil(Stage.width / 2))) {
                success = false;
                part.root._visible = false;
            }
        }
    } catch (error) { success = false; part.root._visible = false; }
    csrEI.call("csr.CombatHudResult", part.name, stamp, success,
        part.root.HudPanel._currentframe, part.root.HudPanel._totalframes);
    return success;
};
_global.csrCombatHud.health = function(stamp, health, armor, helmet, alive) {
    var part = this.part("health");
    if (part == undefined || !part.ready) { return false; }
    var success = false;
    try {
        if (alive && !part.shown) { part.root.showNow(); part.shown = true; }
        var panel = part.root.HudPanel.HealthArmorPanel;
        success = this.write(panel.Health.TextBox, health) &&
            this.write(panel.HealthRed.TextBox, health) && this.write(panel.Armor.TextBox, armor);
        panel.ArmorIcon._visible = armor > 0 && !helmet;
        panel.ArmorHelmetIcon._visible = armor > 0 && helmet;
        panel.HeavyArmorIcon._visible = false;
        success = this.bar(panel.HealthBar.HealthBar, panel.HealthBar.HealthBarRed, health) &&
            this.bar(panel.ArmorBar.ArmorBar, panel.ArmorBar.ArmorBarRed, armor) && success;
    } catch (error) { success = false; }
    return this.result(part, stamp, success, alive === true);
};
_global.csrCombatHud.ammo = function(stamp, clip, reserve, visible) {
    var part = this.part("ammo");
    if (part == undefined || !part.ready) { return false; }
    var success = false;
    try {
        if (visible && !part.shown) { part.root.showNow(); part.shown = true; }
        var panel = part.root.HudPanel.WeaponPanel;
        success = this.write(panel.AmmoCountClip.TextBox, clip) &&
            this.write(panel.AmmoCountTotal.TextBox, "/" + reserve);
        // These original template decorations are not authoritative gameplay
        // values. Keep unsupported subcomponents hidden, rather than display
        // sample bomb, defuser, kill-counter, burst or weapon-name art.
        var hidden = ["KillEater", "WeaponName", "Kill1", "Kill2", "KillCount",
            "Kill_Icon_1", "Kill_Icon_2", "Kill_Icon_3", "Kill_Icon_4", "Kill_Icon_5",
            "UpgradeText", "BurstTypeSingle", "BurstTypeBurst", "Grenades",
            "InBombZoneIcon", "DefuseKitIcon", "BombCarrierIcon", "AmmoAnim", "AmmoAnimFast"];
        var i = 0;
        while (i < hidden.length) { panel[hidden[i]]._visible = false; i += 1; }
    } catch (error) { success = false; }
    return this.result(part, stamp, success, visible === true);
};
_global.csrCombatHud.bar = function(foreground, trail, value) {
    // Public runtime: 100/73/41/19 select widths62/45.2/25.3/11.6;
    // frame1 has no drawn child and zero width. No authored actions inspected.
    if (typeof foreground != "movieclip" || foreground._totalframes != 100 ||
        typeof trail != "movieclip" || trail._totalframes != 100) { return false; }
    var frame = Math.max(1, Math.min(100, Math.floor(value)));
    foreground.gotoAndStop(frame);
    foreground._visible = true;
    trail.gotoAndStop(frame);
    // Damage pulse/trail timing remains uncalibrated. Show steady native art;
    // never leave the template's red layer permanently over the steady fill.
    trail._visible = false;
    return foreground._currentframe == frame;
};
_global.csrCombatHud.resize = function() {
    var i = 0;
    while (i < this.parts.length) {
        var part = this.parts[i];
        if (part.ready) {
            try { part.root.onResize(); }
            catch (error) { this.failed(part, "Resize failed"); }
        }
        i += 1;
    }
};
