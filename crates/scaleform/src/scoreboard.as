// Owned adapter using only installed PlaceObject names / FrameLabel metadata.
// No game instructions or artwork are copied here. Unsupported movies fail closed.
_global.csrScoreboard = new Object();
_global.csrScoreboard.ready = false;
_global.csrScoreboard.freeze = function(clip, depth) {
    if (typeof clip != "movieclip" || depth > 8) { return; }
    clip.stop();
    delete clip.onEnterFrame;
    for (var name in clip) {
        var child = clip[name];
        if (typeof child == "movieclip" && child._parent == clip) {
            this.freeze(child, depth + 1);
        }
    }
};
_global.csrScoreboard.only = function(clip, names) {
    for (var name in clip) {
        var child = clip[name];
        if ((typeof child == "movieclip" || typeof child.getTextFormat == "function") && child._parent == clip) {
            child._visible = names[name] === true;
        }
    }
};
_global.csrScoreboard.write = function(field, value) {
    if (field == undefined || typeof field.getTextFormat != "function") { return false; }
    var format = field.getTextFormat();
    field.text = String(value);
    field.setTextFormat(format);
    field.setNewTextFormat(format);
    field._visible = true;
    if (field.text != String(value)) { return false; }
    return true;
};
_global.csrScoreboard.load = function() {
    var parent = _global.csrResolve("_level9000");
    parent._visible = true;
    this.root = parent.createEmptyMovieClip("csrScoreboard", 210);
    this.root._lockroot = true;
    this.root._visible = false;
    this.loader = new MovieClipLoader();
    this.listener = new Object();
    this.listener.onLoadInit = function(mc) {
        var api = _global.csrScoreboard;
        api.loader.removeListener(this);
        api.freeze(mc, 0);
        mc._visible = false;
        api.ready = typeof mc.ScoreBoard.InnerScoreBoard == "movieclip";
        csrEI.call("csr.ScoreboardReady", api.ready);
    };
    this.listener.onLoadError = function(mc, error) { csrEI.call("csr.ScoreboardReady", false); };
    this.loader.addListener(this.listener);
    this.loader.loadClip("scoreboard.swf", this.root);
};
_global.csrScoreboard.row = function(clip, row) {
    if (typeof clip != "movieclip") { return false; }
    clip._visible = row != undefined;
    if (row == undefined) { return true; }
    clip.gotoAndStop("Up");
    var dead = row[1] == "DEAD";
    clip.Player_Name.gotoAndStop(row[5] ? (dead ? "DeadLocal" : "Local") : (dead ? "Dead" : "Normal"));
    this.freeze(clip, 0);
    this.only(clip, {bg_bar:true, Player_Name:true, Money:true,
        Kills:true, Death:true, Skull:dead, DeadBG:dead,
        PlayerOutline:row[5], Bomb:row[1] == "C4", defuser:row[1] == "KIT"});
    // Unsupported assists/ping/MVP/score are hidden rather than invented.
    // Keep literal return values across the AS2 compiler boundary. A chained
    // method-call return produced undefined despite updating every field.
    var nameOk = this.write(clip.Player_Name.Player_Name, row[0]);
    var moneyOk = this.write(clip.Money, row[2]);
    var killsOk = this.write(clip.Kills, row[3]);
    var deathsOk = this.write(clip.Death, row[4]);
    if (nameOk !== true || moneyOk !== true || killsOk !== true || deathsOk !== true) { return false; }
    return true;
};
_global.csrScoreboard.headers = function(clip, color) {
    this.only(clip, {Player_Label:true, CT_money_header:true,
        CT_Kills_Header:true, CT_Death_Header:true});
    clip._alpha = 100;
    // The frozen labels retain dark template text. Use the corresponding
    // authored roster colour, keeping the original font, size and placement.
    for (var name in clip) {
        var label = clip[name];
        if (typeof label == "movieclip" && label._parent == clip && label._visible) {
            for (var fieldName in label) {
                var field = label[fieldName];
                if (field._parent == label && typeof field.getTextFormat == "function") {
                    field.textColor = color;
                }
            }
        }
    }
};
_global.csrScoreboard.alignScore = function(clip, field, heading) {
    if (typeof clip != "movieclip" || field == undefined || heading == undefined
        || typeof field.getTextFormat != "function"
        || typeof heading.getTextFormat != "function") { return false; }
    // The two score symbols have different registration points. Align their
    // visible text fields. The emblems belong to the background, not these clips.
    // AS2 TextField has no getBounds; transform each text box through its
    // owning MovieClip into the common scoreboard coordinates instead.
    if (!(field._height > 0 && heading._height > 0)) { return false; }
    var scoreCenter = {x:field._x, y:field._y + field._height / 2};
    var headingCenter = {x:heading._x, y:heading._y + heading._height / 2};
    field._parent.localToGlobal(scoreCenter);
    heading._parent.localToGlobal(headingCenter);
    clip._parent.globalToLocal(scoreCenter);
    clip._parent.globalToLocal(headingCenter);
    clip._y += headingCenter.y - scoreCenter.y;
    return true;
};
_global.csrScoreboard.alignEmblem = function(clip, heading) {
    if (typeof clip != "movieclip" || typeof heading.getTextFormat != "function"
        || !(heading._height > 0)) { return false; }
    var bounds = clip.getBounds(clip._parent);
    if (!(bounds.yMax > bounds.yMin)) { return false; }
    var center = {x:heading._x, y:heading._y + heading._height / 2};
    heading._parent.localToGlobal(center);
    clip._parent.globalToLocal(center);
    clip._y += center.y - (bounds.yMin + bounds.yMax) / 2;
    clip._visible = true;
    return true;
};
_global.csrScoreboard.emblems = function(panel) {
    // Both admitted bg_scoreboard symbols place the same unnamed logo group
    // (SWF depth 8). Resolve it by its authored children, not an auto instance name.
    var group;
    for (var name in panel.bg_scoreboard) {
        var child = panel.bg_scoreboard[name];
        if (typeof child == "movieclip" && child._parent == panel.bg_scoreboard
            && typeof child.DefaultIconCT == "movieclip" && child.DefaultIconCT._parent == child
            && typeof child.DefaultIconT == "movieclip" && child.DefaultIconT._parent == child) {
            group = child;
        }
    }
    if (typeof group != "movieclip") { return false; }
    // Custom team-logo placeholders are unused; retain only the original defaults.
    this.only(group, {DefaultIconCT:true, DefaultIconT:true});
    var ctOk = this.alignEmblem(group.DefaultIconCT, panel.CT_TeamName.CT_TeamNameText);
    var tOk = this.alignEmblem(group.DefaultIconT, panel.T_TeamName.T_TeamNameText);
    if (ctOk !== true || tOk !== true) { return false; }
    return true;
};
_global.csrScoreboard.layout = function(panel) {
    // Reuse the authored strip for the second roster. Do not reflow rows,
    // resize backgrounds or add masks; both admitted layouts keep their slots.
    if (typeof panel.csrTHeaders != "movieclip") {
        panel.CT_Headers.duplicateMovieClip("csrTHeaders", 4000);
    }
    if (typeof panel.csrTHeaders != "movieclip") { return false; }
    this.freeze(panel.csrTHeaders, 0);
    panel.csrTHeaders._visible = true;
    panel.csrTHeaders._x = panel.CT_Headers._x;
    panel.csrTHeaders._y = panel.T_Scoretable._y - (panel.CT_Scoretable._y - panel.CT_Headers._y);
    this.headers(panel.CT_Headers, panel.CT_Scoretable.CT_ScoreRow_0.Kills.textColor);
    this.headers(panel.csrTHeaders, panel.T_Scoretable.T_ScoreRow_0.Kills.textColor);
    panel.CT_TeamName._y = panel.CT_Headers._y - panel.CT_TeamName._height - 8;
    var ctOk = this.alignScore(panel.CT_Score, panel.CT_Score.CT_Score, panel.CT_TeamName.CT_TeamNameText);
    var tOk = this.alignScore(panel.T_Score, panel.T_Score.T_Score, panel.T_TeamName.T_TeamNameText);
    var emblemsOk = this.emblems(panel);
    if (ctOk !== true || tOk !== true || emblemsOk !== true) { return false; }
    return true;
};
_global.csrScoreboard.present = function(stamp, title, teams) {
    var success = false;
    var failure = "Authored scoreboard field update rejected";
    try {
        if (!this.ready || teams.length != 2 || teams[0][1].length > 12 || teams[1][1].length > 12) {
            throw "Unsupported scoreboard snapshot";
        }
        var outer = this.root.ScoreBoard;
        outer.gotoAndStop("StartShow");
        var panel = outer.InnerScoreBoard;
        // Retain the layouts admitted in the original r2 runtime capture.
        // Dynamic roster reflow/masking is deferred until separately verified.
        panel.gotoAndStop(Math.max(teams[0][1].length, teams[1][1].length) <= 5 ? "StandardTenPlayer" : "Standard");
        this.freeze(this.root, 0);
        this.only(this.root, {ScoreBoard:true});
        this.only(outer, {InnerScoreBoard:true});
        this.only(panel, {bg_scoreboard:true, CT_TeamName:true, T_TeamName:true,
            CT_Headers:true, csrTHeaders:true, CT_Scoretable:true, T_Scoretable:true, ServerName:true,
            CT_Score:true, T_Score:true});
        success = true;
        if (this.write(panel.ServerName.ServerName, title) !== true) { success = false; }
        if (this.write(panel.CT_TeamName.CT_TeamNameText, teams[0][0]) !== true) { success = false; }
        if (this.write(panel.T_TeamName.T_TeamNameText, teams[1][0]) !== true) { success = false; }
        if (this.write(panel.CT_Score.CT_Score, teams[0][2]) !== true) { success = false; }
        if (this.write(panel.T_Score.T_Score, teams[1][2]) !== true) { success = false; }
        var side = 0;
        while (side < 2) {
            var prefix = side == 0 ? "CT" : "T";
            var table = panel[prefix + "_Scoretable"];
            var i = 0;
            while (i < 12) {
                if (this.row(table[prefix + "_ScoreRow_" + i], teams[side][1][i]) !== true) { success = false; }
                i += 1;
            }
            side += 1;
        }
        outer._alpha = 100;
        panel._alpha = 100;
        // Declarative Standard header placement carries 50% alpha. Keep the
        // original text colours and geometry while restoring legibility.
        panel.CT_Headers._alpha = 100;
        panel.CT_TeamName._alpha = 100;
        panel.T_TeamName._alpha = 100;
        if (this.layout(panel) !== true) { success = false; }
        // Fit original background bounds; invisible awards/vote clips do not
        // affect the viewport. No replacement geometry or synthetic font.
        panel._xscale = 100; panel._yscale = 100; panel._x = 0; panel._y = 0;
        outer._xscale = 100; outer._yscale = 100; outer._x = 0; outer._y = 0;
        var bounds = panel.bg_scoreboard.getBounds(panel);
        var width = bounds.xMax - bounds.xMin;
        var height = bounds.yMax - bounds.yMin;
        if (!(width > 0 && height > 0)) { success = false; }
        var scale = Math.min(Stage.width * 0.50 / width, Stage.height * 0.90 / height);
        panel._xscale = scale * 100; panel._yscale = scale * 100;
        panel._x = (Stage.width - width * scale) / 2 - bounds.xMin * scale;
        panel._y = (Stage.height - height * scale) / 2 - bounds.yMin * scale;
    } catch (error) { success = false; failure = String(error); }
    this.root._visible = success;
    if (!success) { trace("Classic scoreboard presentation failed: " + failure); }
    csrEI.call("csr.ScoreboardResult", stamp, success, success ? "" : failure);
    if (success !== true) { return false; }
    return true;
};
