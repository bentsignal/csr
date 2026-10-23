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
// Original Standard layouts reserve 5/8/12 rows and put the CT score block
// between the teams. Adapt only those existing clips to the visible roster.
// Geometry comes from placement bounds and row pitch, not replacement UI.
_global.csrScoreboard.layout = function(panel, label, ctCount, tCount) {
    if (this.layouts == undefined) { this.layouts = new Object(); }
    var saved = this.layouts[label];
    var ct = panel.CT_Scoretable;
    var t = panel.T_Scoretable;
    var icons;
    // The original background's unnamed avatar holder exposes these named
    // children (declarative sprite 36). Do not depend on generated instance IDs.
    for (var name in panel.bg_scoreboard) {
        var child = panel.bg_scoreboard[name];
        if (typeof child == "movieclip" && child._parent == panel.bg_scoreboard &&
            child.DefaultIconCT != undefined && child.DefaultIconT != undefined) { icons = child; }
    }
    if (saved == undefined) {
        var full = panel.bg_scoreboard.getBounds(panel);
        var capacity = label == "StandardTenPlayer" ? 5 : (label == "StandardSixteenPlayer" ? 8 : 12);
        var last = t["T_ScoreRow_" + (capacity - 1)].getBounds(panel);
        saved = {ctName:panel.CT_TeamName._y, ctScore:panel.CT_Score._y,
            tName:panel.T_TeamName._y, tScore:panel.T_Score._y, table:t._y,
            left:full.xMin, right:full.xMax, top:full.yMin, bottom:full.yMax,
            footer:Math.max(0, full.yMax - last.yMax)};
        if (icons != undefined) { saved.ctIcon = icons.DefaultIconCT._y; saved.tIcon = icons.DefaultIconT._y; }
        this.layouts[label] = saved;
    }
    // Restore before measuring: repeated score updates/resizes cannot accumulate
    // translations, including when gotoAndStop stays on the same frame.
    panel.CT_TeamName._y = saved.ctName; panel.CT_Score._y = saved.ctScore;
    panel.T_TeamName._y = saved.tName; panel.T_Score._y = saved.tScore; t._y = saved.table;
    panel.CT_Headers._alpha = 100;
    panel.CT_TeamName._alpha = 100; panel.T_TeamName._alpha = 100;
    if (icons != undefined) { icons.DefaultIconCT._y = saved.ctIcon; icons.DefaultIconT._y = saved.tIcon; }
    var header = panel.CT_Headers.getBounds(panel);
    var first = ct.CT_ScoreRow_0.getBounds(panel);
    var gap = Math.max(0, first.yMin - header.yMax);
    var ctBottom = Math.max(panel.CT_TeamName.getBounds(panel).yMax, panel.CT_Score.getBounds(panel).yMax);
    var ctDelta = header.yMin - gap - ctBottom;
    panel.CT_TeamName._y += ctDelta; panel.CT_Score._y += ctDelta;
    var ctLast = ct["CT_ScoreRow_" + Math.max(0, ctCount - 1)].getBounds(panel);
    var tTop = Math.min(panel.T_TeamName.getBounds(panel).yMin, panel.T_Score.getBounds(panel).yMin);
    var tDelta = ctLast.yMax + gap - tTop;
    panel.T_TeamName._y += tDelta; panel.T_Score._y += tDelta; t._y += tDelta;
    if (icons != undefined) {
        // Convert the panel-space displacement into the original badge parent.
        var origin = {x:0, y:0}; var point = {x:0, y:ctDelta};
        panel.localToGlobal(origin); panel.localToGlobal(point);
        icons.globalToLocal(origin); icons.globalToLocal(point);
        icons.DefaultIconCT._y += point.y - origin.y;
        origin = {x:0, y:0}; point = {x:0, y:tDelta};
        panel.localToGlobal(origin); panel.localToGlobal(point);
        icons.globalToLocal(origin); icons.globalToLocal(point);
        icons.DefaultIconT._y += point.y - origin.y;
    }
    var tLast = t["T_ScoreRow_" + Math.max(0, tCount - 1)].getBounds(panel);
    var bottom = Math.min(saved.bottom, tLast.yMax + saved.footer);
    // Clip unused bottom space without stretching/repainting original artwork.
    var mask = panel.__csrRosterMask;
    if (mask == undefined) { mask = panel.createEmptyMovieClip("__csrRosterMask", panel.getNextHighestDepth()); }
    mask.clear(); mask.beginFill(0xffffff, 100);
    mask.moveTo(saved.left, saved.top); mask.lineTo(saved.right, saved.top);
    mask.lineTo(saved.right, bottom); mask.lineTo(saved.left, bottom);
    mask.lineTo(saved.left, saved.top); mask.endFill();
    panel.bg_scoreboard.setMask(mask);
    return {xMin:saved.left, xMax:saved.right, yMin:saved.top, yMax:bottom,
        originalHeight:saved.bottom - saved.top};
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
        var count = Math.max(teams[0][1].length, teams[1][1].length);
        var layout = count <= 5 ? "StandardTenPlayer" : (count <= 8 ? "StandardSixteenPlayer" : "Standard");
        panel.gotoAndStop(layout);
        this.freeze(this.root, 0);
        this.only(this.root, {ScoreBoard:true});
        this.only(outer, {InnerScoreBoard:true});
        this.only(panel, {bg_scoreboard:true, CT_TeamName:true, T_TeamName:true,
            CT_Headers:true, CT_Scoretable:true, T_Scoretable:true, ServerName:true,
            CT_Score:true, T_Score:true});
        this.only(panel.CT_Headers, {Player_Label:true, CT_money_header:true,
            CT_Kills_Header:true, CT_Death_Header:true});
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
        // Fit original background bounds; invisible awards/vote clips do not
        // affect the viewport. No replacement geometry or synthetic font.
        panel._xscale = 100; panel._yscale = 100; panel._x = 0; panel._y = 0;
        outer._xscale = 100; outer._yscale = 100; outer._x = 0; outer._y = 0;
        var bounds = this.layout(panel, layout, teams[0][1].length, teams[1][1].length);
        if (bounds == undefined) { throw "Unsupported scoreboard layout"; }
        var width = bounds.xMax - bounds.xMin;
        var height = bounds.yMax - bounds.yMin;
        if (!(width > 0 && height > 0)) { success = false; }
        var scale = Math.min(Stage.width * 0.94 / width, Stage.height * 0.90 / bounds.originalHeight);
        panel._xscale = scale * 100; panel._yscale = scale * 100;
        panel._x = (Stage.width - width * scale) / 2 - bounds.xMin * scale;
        panel._y = (Stage.height - height * scale) / 2 - bounds.yMin * scale;
    } catch (error) { success = false; }
    this.root._visible = success;
    csrEI.call("csr.ScoreboardResult", stamp, success);
    if (success !== true) { return false; }
    return true;
};
