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
            child._visible = names[":" + name] === true;
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
    return field.text == String(value);
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
    this.only(clip, {":bg_bar":true, ":Player_Name":true, ":Money":true,
        ":Kills":true, ":Death":true, ":Skull":dead, ":DeadBG":dead,
        ":PlayerOutline":row[5], ":Bomb":row[1] == "C4", ":defuser":row[1] == "KIT"});
    // Unsupported assists/ping/MVP/score are hidden rather than invented.
    return this.write(clip.Player_Name.Player_Name, row[0]) &&
        this.write(clip.Money, row[2]) && this.write(clip.Kills, row[3]) && this.write(clip.Death, row[4]);
};
_global.csrScoreboard.present = function(stamp, title, teams) {
    var success = false;
    try {
        if (!this.ready || teams.length != 2 || teams[0][1].length > 12 || teams[1][1].length > 12) {
            throw "Unsupported scoreboard snapshot";
        }
        var outer = this.root.ScoreBoard;
        outer.gotoAndStop("StartShow");
        var panel = outer.InnerScoreBoard;
        panel.gotoAndStop(Math.max(teams[0][1].length, teams[1][1].length) <= 5 ? "StandardTenPlayer" : "Standard");
        this.freeze(this.root, 0);
        this.only(this.root, {":ScoreBoard":true});
        this.only(outer, {":InnerScoreBoard":true});
        this.only(panel, {":bg_scoreboard":true, ":CT_TeamName":true, ":T_TeamName":true,
            ":CT_Headers":true, ":CT_Scoretable":true, ":T_Scoretable":true, ":ServerName":true,
            ":CT_Score":true, ":T_Score":true});
        this.only(panel.CT_Headers, {":Player_Label":true, ":CT_money_header":true,
            ":CT_Kills_Header":true, ":CT_Death_Header":true});
        success = this.write(panel.ServerName.ServerName, title) &&
            this.write(panel.CT_TeamName.CT_TeamNameText, teams[0][0]) &&
            this.write(panel.T_TeamName.T_TeamNameText, teams[1][0]) &&
            this.write(panel.CT_Score.CT_Score, teams[0][2]) && this.write(panel.T_Score.T_Score, teams[1][2]);
        var side = 0;
        while (side < 2) {
            var prefix = side == 0 ? "CT" : "T";
            var table = panel[prefix + "_Scoretable"];
            var i = 0;
            while (i < 12) {
                success = this.row(table[prefix + "_ScoreRow_" + i], teams[side][1][i]) && success;
                i += 1;
            }
            side += 1;
        }
        outer._alpha = 100;
        panel._alpha = 100;
        // Fit original background bounds; invisible awards/vote clips do not
        // affect the viewport. No replacement geometry or synthetic font.
        panel._xscale = 100; panel._yscale = 100; panel._x = 0; panel._y = 0;
        outer._xscale = 100; outer._yscale = 100; outer._x = 0; outer._y = 0;
        var bounds = panel.bg_scoreboard.getBounds(panel);
        var width = bounds.xMax - bounds.xMin;
        var height = bounds.yMax - bounds.yMin;
        if (!(width > 0 && height > 0)) { success = false; }
        var scale = Math.min(Stage.width * 0.94 / width, Stage.height * 0.90 / height);
        panel._xscale = scale * 100; panel._yscale = scale * 100;
        panel._x = (Stage.width - width * scale) / 2 - bounds.xMin * scale;
        panel._y = (Stage.height - height * scale) / 2 - bounds.yMin * scale;
    } catch (error) { success = false; }
    this.root._visible = success;
    csrEI.call("csr.ScoreboardResult", stamp, success);
    return success;
};
