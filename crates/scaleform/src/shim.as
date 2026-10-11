// CSGO Rewrite Scaleform bridge (our own code, compiled at runtime by rascal).
//
// Plays the role of the engine side of CS:GO's Scaleform integration: it
// defines the native objects the menu movies call (`_global.GameInterface`,
// `_global.CScaleformComponent_*`, each element's `gameAPI`) as proxies that
// forward every method call to the host through ExternalInterface, loads the
// root movie, and lets the host call back into ActionScript.

var csrEI = flash.external.ExternalInterface;
_global.csrHandles = [];

// Values that cannot cross ExternalInterface (movie clips, objects, functions)
// are passed to the host as handles it can hand back later.
_global.csrToHost = function(v) {
    var t = typeof v;
    if (t == "movieclip") {
        return "mc:" + String(v);
    }
    if (t == "object" && v != null && !(v instanceof Array)) {
        _global.csrHandles.push(v);
        return "obj:" + (_global.csrHandles.length - 1);
    }
    if (t == "function") {
        _global.csrHandles.push(v);
        return "fn:" + (_global.csrHandles.length - 1);
    }
    return v;
};

_global.csrMakeProxy = function(prefix) {
    var proxy = new Object();
    proxy.__resolve = function(name) {
        var method = function() {
            var a = [prefix + "." + name];
            var i = 0;
            while (i < arguments.length) {
                a.push(_global.csrToHost(arguments[i]));
                i = i + 1;
            }
            return csrEI.call.apply(csrEI, a);
        };
        return method;
    };
    return proxy;
};

_global.csrApis = new Object();
_global.csrApi = function(name) {
    if (_global.csrApis[name] == undefined) {
        _global.csrApis[name] = _global.csrMakeProxy(name);
    }
    return _global.csrApis[name];
};

// Resolves "_global.MainMenuMovie.Panel" or "_level31.Panel" to an object.
_global.csrResolve = function(path) {
    var parts = path.split(".");
    // A bare `_global` value evaluates to undefined here (only member access
    // like `_global.x` works when Ruffle calls in through ExternalInterface),
    // so "_global.A..." starts the walk at `_global[A]`.
    var o;
    var i;
    if (parts[0] == "_global") {
        o = _global[parts[1]];
        i = 2;
    } else {
        o = eval(parts[0]);
        i = 1;
    }
    while (i < parts.length && o != undefined) {
        o = o[parts[i]];
        i = i + 1;
    }
    return o;
};

csrEI.addCallback("csrCall", null, function(path, fname, args) {
    var o = _global.csrResolve(path);
    if (o == undefined) {
        return "csr:missing-target";
    }
    var f = o[fname];
    if (typeof f != "function") {
        return "csr:missing-function";
    }
    if (args == undefined) {
        args = [];
    }
    return _global.csrToHost(f.apply(o, args));
});

csrEI.addCallback("csrSet", null, function(path, name, value) {
    var o = _global.csrResolve(path);
    if (o != undefined) {
        o[name] = value;
    }
});

csrEI.addCallback("csrGet", null, function(path) {
    return _global.csrToHost(_global.csrResolve(path));
});

csrEI.addCallback("csrRequestElement", null, function(elementId, apiName) {
    _global.RequestElement(elementId, _global.csrApi(apiName));
});

// Keys reached the movies through _global.KeyDownEvent(code, vkey, slot,
// binding); Lib.SFKey turns codes into Source key names with the engine's
// _global.ValveKeyTable (both directions). The movies only ever look names
// up, so our own numbering works.
_global.ValveKeyTable = new Object();
var csrKeyNames = ["KEY_0", "KEY_1", "KEY_2", "KEY_3", "KEY_4", "KEY_5", "KEY_6", "KEY_7",
    "KEY_8", "KEY_9", "KEY_A", "KEY_B", "KEY_C", "KEY_D", "KEY_E", "KEY_F", "KEY_G", "KEY_H",
    "KEY_I", "KEY_J", "KEY_K", "KEY_L", "KEY_M", "KEY_N", "KEY_O", "KEY_P", "KEY_Q", "KEY_R",
    "KEY_S", "KEY_T", "KEY_U", "KEY_V", "KEY_W", "KEY_X", "KEY_Y", "KEY_Z", "KEY_PAD_0",
    "KEY_PAD_1", "KEY_PAD_2", "KEY_PAD_3", "KEY_PAD_4", "KEY_PAD_5", "KEY_PAD_6", "KEY_PAD_7",
    "KEY_PAD_8", "KEY_PAD_9", "KEY_PAD_ENTER", "KEY_ENTER", "KEY_SPACE", "KEY_BACKSPACE",
    "KEY_TAB", "KEY_ESCAPE", "KEY_INSERT", "KEY_DELETE", "KEY_HOME", "KEY_END", "KEY_PAGEUP",
    "KEY_PAGEDOWN", "KEY_LSHIFT", "KEY_RSHIFT", "KEY_LCONTROL", "KEY_RCONTROL", "KEY_LALT",
    "KEY_RALT", "KEY_UP", "KEY_LEFT", "KEY_DOWN", "KEY_RIGHT", "MOUSE_LEFT", "MOUSE_RIGHT",
    "MOUSE_MIDDLE", "MOUSE_WHEEL_UP", "MOUSE_WHEEL_DOWN"];
var csrKeyIndex = 0;
while (csrKeyIndex < csrKeyNames.length) {
    _global.ValveKeyTable[csrKeyIndex + 1] = csrKeyNames[csrKeyIndex];
    _global.ValveKeyTable[csrKeyNames[csrKeyIndex]] = csrKeyIndex + 1;
    csrKeyIndex = csrKeyIndex + 1;
}

csrEI.addCallback("csrKey", null, function(name, vkey, down) {
    var code = _global.ValveKeyTable[name];
    if (code == undefined) {
        return false;
    }
    if (down) {
        return _global.KeyDownEvent(code, vkey, 0, undefined);
    }
    return _global.KeyUpEvent(code, vkey, 0, undefined);
});

// Component names come from the host (the list it knows how to serve).
csrEI.addCallback("csrDefineComponents", null, function(names) {
    var i = 0;
    while (i < names.length) {
        _global[names[i]] = _global.csrMakeProxy(names[i]);
        i = i + 1;
    }
});

// Scaleform Stage extensions read by Lib.ResizeManager: the authored stage
// (1280x720 for every menu movie) and the visible area in pixels.
Stage.originalRect = {left: 0, top: 0, width: 1280, height: 720};
Stage.addProperty("visibleRect", function() {
    return {left: 0, top: 0, width: Stage.width, height: Stage.height};
}, null);

// avatar.swf hides DynamicAvatar right before loading `img://avatar_<xuid>`
// into it and never shows it again, yet the reference shows avatars:
// Scaleform's load left the clip visible. Emulate that for avatar loads only.
var csrLoadClip = MovieClipLoader.prototype.loadClip;
MovieClipLoader.prototype.loadClip = function(url, target) {
    if (String(url).indexOf("img://avatar_") == 0) {
        var shower = new Object();
        shower.onLoadInit = function(mc) {
            mc._visible = true;
        };
        this.addListener(shower);
    }
    return csrLoadClip.apply(this, arguments);
};

_global.GameInterface = _global.csrMakeProxy("GameInterface");
_global.PlatformCode = 0;
_global.UISlot = 0;
_global.wantControllerShown = false;

loadMovieNum(csrRootMovie, csrRootLevel);

this.onEnterFrame = function() {
    if (_global.RequestElement != undefined) {
        delete this.onEnterFrame;
        csrEI.call("csr.RootReady");
    }
};
