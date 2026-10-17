// Owned host composition, never extracted authored instructions. All HUD
// adapters share the recorded hud_scaling setting and the 720-high stage.
_global.csrHudLayout = new Object();
_global.csrHudLayout.scale = function() {
    var value = Number(csrEI.call("GameInterface.GetConvarNumber", "hud_scaling"));
    return value > 0 && value <= 1 ? value : 1;
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
    // Player strip40 + separation12 authored units: retained 2015 first-row
    // outline starts near66px at1080. Row size/pitch remain movie-owned.
    root.Panel._y = this.unit(52);
};
