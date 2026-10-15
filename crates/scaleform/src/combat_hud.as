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
