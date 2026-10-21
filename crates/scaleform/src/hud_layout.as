// Owned host composition, never extracted authored instructions. All HUD
// adapters share the recorded hud_scaling setting and the 720-high stage.
// The client supplies its compact-HUD correction for the authored modules.
_global.csrHudLayout = new Object();
_global.csrHudLayout.scale = function() {
    var value = Number(csrEI.call("GameInterface.GetConvarNumber", "hud_scaling"));
    var compact = Number(csrEI.call("csr.CompactHudScale"));
    if (!(compact > 0) || compact > 1) { compact = 1; }
    if (value <= 0 || value > 1) { value = 1; }
    return value * compact;
};
_global.csrHudLayout.unit = function(value) {
    return value * this.scale() * Stage.height / Stage.originalRect.height;
};
_global.csrHudLayout.bottom = function(clip, right) {
    if (typeof clip != "movieclip") { return false; }
    // Reset before the movie computes viewport geometry. Never measure/resize
    // content bounds: those vary with ammo digits, animations and helper clips.
    clip._xscale = 100;
    clip._yscale = 100;
    clip._x = 0;
    clip._y = 0;
    clip.onResize();
    var scale = this.scale();
    clip._xscale = scale * 100;
    clip._yscale = scale * 100;
    clip._x = right ? Stage.width * (1 - scale) : 0;
    clip._y = Stage.height * (1 - scale);
    return true;
};
_global.csrHudLayout.notices = function(root) {
    // Player strip40 + separation12 authored units. The client anchors the
    // tight notice crop and applies its compact scale once to that raster;
    // row size/pitch remain movie-owned here.
    root.Panel._y = this.unit(52);
};
