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

// An explicit host-side clip reference is distinct from a player's literal
// string. Public HUD setters accept the clip returned by AddPanel.
_global.csrFromHost = function(v) {
    if (typeof v == "object" && v != null && typeof v.__csrMovieClip == "string") {
        var clip = _global.csrResolve(v.__csrMovieClip);
        return typeof clip == "movieclip" ? clip : undefined;
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

// Element movies can overwrite their API aliases while loading. Bind the
// native object after loading, before initializing the authored dialog.
_global.csrBindApi = function(alias, name) {
    _global[alias] = _global.csrApi(name);
};
_global.csrBindings = new Object();
_global.csrBindings.bindApi = _global.csrBindApi;

// Our passive surface bookkeeping, not a game callback. The owner/epoch/serial
// and registered loader paths are echoed literally; the engine rejects stale
// responses. Read only standard Flash properties, never authored instructions.
_global.csrBindings.samplePauseSurface = function(owner, epoch, serial, pausePath, optionsPath) {
    var pause = _global.csrResolve(pausePath);
    var options = _global.csrResolve(optionsPath);
    var pauseExists = typeof pause == "movieclip" && String(pause) == pausePath;
    var optionsExists = optionsPath != "" && typeof options == "movieclip" && String(options) == optionsPath;
    var pauseVisible;
    var pauseAlpha;
    var optionsVisible;
    var optionsAlpha;
    if (pauseExists) {
        pauseVisible = pause._visible;
        pauseAlpha = pause._alpha;
    }
    if (optionsExists) {
        optionsVisible = options._visible;
        optionsAlpha = options._alpha;
    }
    csrEI.call("OwnedPauseSurface.Visibility", owner, epoch, serial, pausePath, optionsPath,
        pauseExists, pauseVisible, pauseAlpha, optionsExists, optionsVisible, optionsAlpha);
};

// Our epoch labels identify stale owned-menu callbacks. Keep only the live
// alias, without placing each reopen generation in the ordinary API cache.
_global.csrBindings.validOwnedName = function(name, prefix) {
    if (typeof name != "string" || name.indexOf(prefix) != 0) {
        return false;
    }
    var digits = name.substr(prefix.length);
    if (digits.length == 0 || digits.length > 20 ||
        (digits.length > 1 && digits.charAt(0) == "0")) {
        return false;
    }
    var max = "18446744073709551615";
    var comparison = 0;
    var i = 0;
    while (i < digits.length) {
        var code = digits.charCodeAt(i);
        if (code < 48 || code > 57) {
            return false;
        }
        if (digits.length == 20 && comparison == 0) {
            var maxCode = max.charCodeAt(i);
            if (code > maxCode) {
                comparison = 1;
            } else if (code < maxCode) {
                comparison = -1;
            }
        }
        i = i + 1;
    }
    if (comparison > 0) {
        return false;
    }
    return true;
};
_global.csrBindings.bindOwnedApi = function(alias, name) {
    var prefix;
    if (alias == "LobbyAPI") { prefix = "OwnedLobby"; }
    else if (alias == "SinglePlayerAPI") { prefix = "OwnedPicker"; }
    else { return false; }
    if (!_global.csrBindings.validOwnedName(name, prefix)) { return false; }
    _global[alias] = _global.csrMakeProxy(name);
    return true;
};

// The installed dialog retains the engine proxy passed to RequestElement;
// rebinding root.gameAPI does not replace that reference. Change only the
// observed button callback on OUR cached proxy, with an immutable opening
// receiver. Other native getters/timer callbacks keep the loader API.
_global.csrBindings.bindOwnedQuit = function(path, name) {
    var root = _global.csrResolve(path);
    var api = _global.csrApi("MessageBox");
    if (path != "_level500" || root == undefined || root.elementName != "MessageBox" ||
        root.gameAPI != api || !_global.csrBindings.validOwnedName(name, "OwnedQuit")) { return false; }
    if (root.csrQuitBoundAPI == undefined) {
        root.csrQuitBoundAPI = api;
        root.csrQuitHadButton = api.hasOwnProperty("OnButtonPress");
        root.csrQuitBaseButton = api.OnButtonPress;
    }
    var owned = _global.csrMakeProxy(name);
    var callback = function(code) {
        if (arguments.length != 1 || typeof code != "number") { return; }
        // Both pixels were observed: native OK -> our KEY_ENTER48;
        // Cancel -> our KEY_ESCAPE52. Keep our table, not magic native values.
        var key = _global.ValveKeyTable[code];
        if (key != "KEY_ENTER" && key != "KEY_ESCAPE") { return; }
        return owned.OnButtonPress(key);
    };
    root.csrQuitCallback = callback;
    api.OnButtonPress = callback;
    return true;
};
_global.csrBindings.releaseOwnedQuit = function(path) {
    var root = _global.csrResolve(path);
    if (path != "_level500" || root == undefined || root.elementName != "MessageBox" ||
        typeof root.csrQuitBoundAPI != "object") { return false; }
    var api = root.csrQuitBoundAPI;
    if (api.OnButtonPress == root.csrQuitCallback) {
        if (root.csrQuitHadButton) { api.OnButtonPress = root.csrQuitBaseButton; }
        else { delete api.OnButtonPress; }
    }
    delete root.csrQuitBoundAPI;
    delete root.csrQuitBaseButton;
    delete root.csrQuitHadButton;
    delete root.csrQuitCallback;
    return true;
};

// Bind the observed Options close/layout callbacks on our cached loader proxy.
// Retained function references keep their old receiver. A retained API object
// reading these properties after rebind sees the NEW receiver; synthetic tests
// distinguish both cases, but do not establish the installed movie's pattern.
_global.csrBindings.bindOwnedOptions = function(path, name) {
    if (typeof path != "string" || path != "_level21" ||
        !_global.csrBindings.validOwnedName(name, "OwnedOptions")) { return false; }
    var root = _global.csrResolve(path);
    var api = _global.csrApi("OptionsMenu");
    if (typeof root != "movieclip" || String(root) != path ||
        root.elementName != "OptionsMenu" || root.gameAPI != api) { return false; }
    var owned = _global.csrMakeProxy(name);
    api.OnCancel = owned.OnCancel;
    api.OnUnload = owned.OnUnload;
    api.OnLayoutComplete = owned.OnLayoutComplete;
    return true;
};
// The host consumes match Escape once. It sends the immutable receiver captured
// on that press, so queued cancellation cannot close a replacement opening.
_global.csrBindings.cancelOwnedOptions = function(name) {
    if (_global.csrBindings.validOwnedName(name, "OwnedOptions")) {
        csrEI.call(name + ".OnCancel");
    }
};

// Our cached-root ownership adapter tags only unload. Other loader callbacks
// retain their original API receiver. A retained wrapper keeps its own epoch;
// an authored action that reads the newly rebound property uses the new epoch.
_global.csrBindings.bindOwnedLifecycle = function(path, name) {
    var alias;
    var element;
    if (path == "_level16") {
        alias = "LobbyAPI";
        element = "LobbyScreen";
    } else if (path == "_level17") {
        alias = "LobbyAPI";
        element = "LobbyBrowser";
    } else if (path == "_level15") {
        alias = "SinglePlayerAPI";
        element = "SinglePlayer";
    } else {
        return false;
    }
    var root = _global.csrResolve(path);
    if (root == undefined || root.elementName != element ||
        typeof root.gameAPI != "object" ||
        !_global.csrBindings.bindOwnedApi(alias, name)) {
        return false;
    }
    if (root.csrOwnedLifecycleBase == undefined) {
        root.csrOwnedLifecycleBase = root.gameAPI;
    }
    var base = root.csrOwnedLifecycleBase;
    var owned = _global[alias];
    var wrapper = new Object();
    wrapper.__resolve = function(method) {
        if (method == "OnUnload") {
            return owned[method];
        }
        return base[method];
    };
    root.gameAPI = wrapper;
    return true;
};

// Return a cached element to its ordinary loader API before another flow
// reuses it. Retained old wrappers still carry their old epoch.
_global.csrBindings.releaseOwnedLifecycle = function(path) {
    if (path != "_level15" && path != "_level16" && path != "_level17") {
        return false;
    }
    var root = _global.csrResolve(path);
    if (root == undefined || typeof root.csrOwnedLifecycleBase != "object") {
        return false;
    }
    root.gameAPI = root.csrOwnedLifecycleBase;
    delete root.csrOwnedLifecycleBase;
    return true;
};

// The authored options callbacks can lose their row index after mouse updates.
// Resolve the validated row to the same clip used by the native ShowPanel path.
_global.csrBindings.focusWidget = function(path, index) {
    var root = _global.csrResolve(path);
    var panel = root.TopPanel.Panel;
    var widget = panel.arraySlots[index].Widget;
    if (widget != undefined && typeof _global.navManager.SetHighlightedObject == "function") {
        panel.dialog.m_nCurrentWidget = index;
        _global.navManager.SetHighlightedObject(widget);
    }
};

// Arm an axis correction from the exact registered pip's own native event.
// Callbacks remain authored; the host retains Flash's original drag machinery.
_global.csrBindings.wrapSliderDragEvent = function(pip, widget, axis, name) {
    var base = pip[name];
    if (typeof base != "function") {
        return;
    }
    pip[name] = function() {
        var result = base.apply(this, arguments);
        if (this == pip && widget.m_bDraggingMouse === true) {
            csrEI.call("csr.RegisterSliderDragAxis", "mc:" + String(pip), axis);
        }
        return result;
    };
};
_global.csrBindings.preserveSliderDragAxis = function(path) {
    var widget = _global.csrResolve(path);
    var pip = widget.ScrollPip;
    if (typeof pip != "movieclip" || typeof widget.m_nSlideMin != "number" ||
        typeof widget.m_nSlideMax != "number" || !isFinite(pip._y)) {
        return false;
    }
    if (pip.csrSliderAxisInstalled === true) {
        return true;
    }
    pip.csrSliderAxisInstalled = true;
    var axis = pip._y;
    _global.csrBindings.wrapSliderDragEvent(pip, widget, axis, "onPress");
    _global.csrBindings.wrapSliderDragEvent(pip, widget, axis, "onMouseMove");
    return true;
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

_global.csrProbeBudget = 0;
_global.csrProbeTrace = function(kind, path, typeName) {
    if (_global.csrProbeBudget > 0) {
        _global.csrProbeBudget = _global.csrProbeBudget - 1;
        csrEI.call("csr.Probe" + kind, path, typeName);
    }
};
_global.csrProbeProperty = function(target, key, value, path) {
    target.addProperty(key, function() {
        _global.csrProbeTrace("Read", path, typeof value);
        return value;
    }, null);
};
_global.csrProbeRecord = function(value, path, depth) {
    if (typeof value != "object" || value == null || depth > 8) {
        return value;
    }
    var result = value instanceof Array ? [] : new Object();
    for (var key in value) {
        if (value.hasOwnProperty(key)) {
            var childPath = path + "." + key;
            var child = _global.csrProbeRecord(value[key], childPath, depth + 1);
            _global.csrProbeProperty(result, key, child, childPath);
        }
    }
    if (value instanceof Array) {
        result.length = value.length;
    }
    result.__resolve = function(key) {
        _global.csrProbeTrace("Missing", path + "." + key, "undefined");
        return undefined;
    };
    return result;
};

// Explicit probe-only observation of absent runtime member lookups. Existing
// resolvers are preserved, and absent members still resolve to undefined.
_global.csrDiagnostics = new Object();
// Observe public call contracts without reading authored instructions. Retain
// a returned clip only in this explicitly invoked diagnostic namespace.
_global.csrDiagnostics.observeCall = function(path, name, args) {
    var target = _global.csrResolve(path);
    if (target == undefined || typeof target[name] != "function") {
        return false;
    }
    if (args == undefined) { args = []; }
    var i = 0;
    while (i < args.length) {
        args[i] = _global.csrFromHost(args[i]);
        i += 1;
    }
    this.lastResult = target[name].apply(target, args);
    csrEI.call("csr.PublicCallResult", path, name, typeof this.lastResult,
        _global.csrToHost(this.lastResult));
    return true;
};
_global.csrDiagnostics.observeRecord = function(path) {
    var split = path.lastIndexOf(".");
    var parent = _global.csrResolve(path.substr(0, split));
    var key = path.substr(split + 1);
    if (parent == undefined || typeof parent[key] != "object" || parent[key] == null) { return false; }
    _global.csrProbeBudget = 128;
    parent[key] = _global.csrProbeRecord(parent[key], path, 0);
    return true;
};
_global.csrDiagnostics.observeGlobal = function() {
    var previousResolver = _global.__resolve;
    _global.csrProbeBudget = 128;
    _global.__resolve = function(key) {
        _global.csrProbeTrace("Missing", "_global." + key, "undefined");
        if (typeof previousResolver == "function") {
            return previousResolver.apply(this, [key]);
        }
        return undefined;
    };
    return true;
};
_global.csrDiagnostics.observeTarget = function(path) {
    var target = _global.csrResolve(path);
    if (target == undefined) {
        return false;
    }
    var previousResolver = target.__resolve;
    _global.csrProbeBudget = 128;
    target.__resolve = function(key) {
        _global.csrProbeTrace("Missing", path + "." + key, "undefined");
        if (typeof previousResolver == "function") {
            return previousResolver.apply(this, [key]);
        }
        return undefined;
    };
    return true;
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
    var i = 0;
    while (i < args.length) {
        args[i] = _global.csrFromHost(args[i]);
        if (typeof args[i] == "object" && args[i] != null && args[i].__csrProbe === true) {
            _global.csrProbeBudget = 128;
            args[i] = _global.csrProbeRecord(args[i].value, path + "." + fname + ".arg" + i, 0);
        }
        i = i + 1;
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

// Read-only runtime API discovery for integration diagnostics. Exposes names
// and value types, never function bodies or movie bytecode.
csrEI.addCallback("csrInspect", null, function(path) {
    var o = _global.csrResolve(path);
    var entries = [];
    if (typeof o == "function") {
        entries.push("length:number=" + o.length);
    }
    if (typeof o == "movieclip") {
        var localBounds = o.getBounds(o);
        var bounds = o._parent == undefined ? localBounds : o.getBounds(o._parent);
        entries.push("_x:number=" + o._x, "_y:number=" + o._y,
            "_width:number=" + o._width, "_height:number=" + o._height,
            "_xscale:number=" + o._xscale, "_yscale:number=" + o._yscale,
            "boundsInParent:string=" + [bounds.xMin, bounds.yMin, bounds.xMax, bounds.yMax].join(","),
            "boundsLocal:string=" + [localBounds.xMin, localBounds.yMin, localBounds.xMax, localBounds.yMax].join(","));
    }
    for (var key in o) {
        var t = typeof o[key];
        var primitive = t == "number" || t == "boolean" || t == "string" || o[key] == null;
        entries.push(key + ":" + t + (primitive ? "=" + String(o[key]).substr(0, 256) : ""));
    }
    return entries;
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

// Authored numeric fields are dynamic TextFields. Their editor receives
// character strings through the installed movie's CharTyped event.
csrEI.addCallback("csrChar", null, function(typed) {
    if (typeof _global.CharTyped != "function") {
        return false;
    }
    return _global.CharTyped(typed, 0);
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
// Explicit compatibility surface: use the selected map's installed radar,
// not the unrelated modern full-screen photo, in the authored overview box.
// The frontend owns the generation and clears it on Home/cancellation.
_global.csrLoadingOverviews = new Object();
_global.csrBindings.configureLoadingOverview = function(path, map, serial, radarOnly, ownedRadarUrl, ownedIconUrl) {
    var previous = _global.csrLoadingOverviews[path];
    if (previous != undefined && previous.map == map && previous.serial == serial) { return true; }
    delete _global.csrLoadingOverviews[path];
    if (previous != undefined && previous.loader != undefined) {
        previous.loader.removeListener(previous.listener);
        previous.loader.unloadClip(previous.target);
    }
    if (radarOnly !== true || typeof map != "string" || map.length == 0 || map.length > 128) { return false; }
    var root = _global.csrResolve(path);
    var box = root.Panel.OverviewBG;
    if (box == undefined || box._width <= 0 || box._height <= 0 ||
        box._width > 4096 || box._height > 4096) { return false; }
    _global.csrLoadingOverviews[path] = {map: map, serial: serial,
        target: root.Panel.Overview, x: box._x, y: box._y, width: box._width, height: box._height,
        ownedRadarUrl: ownedRadarUrl, ownedIconUrl: ownedIconUrl};
    return true;
};
var csrLoadClip = MovieClipLoader.prototype.loadClip;
MovieClipLoader.prototype.loadClip = function(url, target) {
    // A selected archive owns opaque engine images. Only this root's exact
    // authored collection-icon URL is redirected; there is no general alias.
    for (var ownerPath in _global.csrLoadingOverviews) {
        var owner = _global.csrLoadingOverviews[ownerPath];
        if (typeof owner.ownedIconUrl == "string" &&
            String(target).indexOf(ownerPath + ".") == 0 &&
            String(url) == "images/map_icons/collection_icon_" + owner.map + ".png") {
            url = owner.ownedIconUrl;
            arguments[0] = url;
            break;
        }
    }
    var rootPath = String(target).split(".Panel.Overview")[0];
    var context = _global.csrLoadingOverviews[rootPath];
    if (context != undefined && String(target) == rootPath + ".Panel.Overview" &&
        String(url) == "../../maps/" + context.map + "_loading.jpg") {
        // The owned archive route supplies its verified embedded older radar.
        // Ordinary installed maps keep the previous normal-radar compatibility.
        url = typeof context.ownedRadarUrl == "string" ? context.ownedRadarUrl :
            "../overviews/" + context.map + "_radar.dds";
        arguments[0] = url;
        if (context.loader != undefined) {
            context.loader.removeListener(context.listener);
            context.loader.unloadClip(context.target);
        }
        var fit = new Object();
        fit.context = context;
        fit.path = rootPath;
        fit.loader = this;
        fit.onLoadInit = function(mc) {
            if (this.traceUrl != undefined) {
                csrEI.call("csr.LoadingArtInit", this.traceUrl, this.traceRequested, String(mc), mc._width, mc._height);
            }
            this.loader.removeListener(this);
            if (_global.csrLoadingOverviews[this.path] != this.context ||
                this.context.listener != this ||
                mc != this.context.target || mc._width <= 0 || mc._height <= 0) { return; }
            // Preserve aspect and source resolution; fit in measured authored
            // parent coordinates after the movie's own initialization.
            var scale = Math.min(this.context.width / mc._width, this.context.height / mc._height);
            mc._xscale *= scale;
            mc._yscale *= scale;
            mc._x = this.context.x + (this.context.width - mc._width) / 2;
            mc._y = this.context.y + (this.context.height - mc._height) / 2;
            // Production readiness follows the real, current fit's Init.
            csrEI.call("csr.LoadingOverviewReady", this.context.map, this.context.serial, String(mc));
        };
        fit.onLoadError = function(mc, error) {
            if (this.traceUrl != undefined) {
                csrEI.call("csr.LoadingArtError", this.traceUrl, this.traceRequested, String(mc), String(error));
            }
            this.loader.removeListener(this);
        };
        context.loader = this;
        context.listener = fit;
        this.addListener(fit);
    }
    // Opt-in public loader observation for the existing menu probe only.
    if (_global.csrDiagnostics.loadingTraceBudget > 0 &&
        (String(url).indexOf("_loading.jpg") >= 0 || String(url).indexOf("_radar") >= 0 ||
         String(url).indexOf("img://loading/") == 0 || String(url).indexOf("collection_icon_") >= 0)) {
        _global.csrDiagnostics.loadingTraceBudget -= 1;
        csrEI.call("csr.LoadingArtRequest", String(url), String(target));
        if (fit != undefined) {
            // The fit removes itself during Init. A second listener can be
            // skipped when a broadcaster walks its live indexed listener list.
            // Forward the real callbacks from that same listener instead.
            fit.traceUrl = String(url);
            fit.traceRequested = String(target);
            fit.onLoadComplete = function(mc) {
                csrEI.call("csr.LoadingArtComplete", this.traceUrl, this.traceRequested, String(mc), mc._width, mc._height);
            };
        } else {
            var observer = new Object();
            observer.loader = this;
            observer.url = String(url);
            observer.requested = String(target);
            observer.onLoadComplete = function(mc) {
                csrEI.call("csr.LoadingArtComplete", this.url, this.requested, String(mc), mc._width, mc._height);
            };
            observer.onLoadInit = function(mc) {
                csrEI.call("csr.LoadingArtInit", this.url, this.requested, String(mc), mc._width, mc._height);
                this.loader.removeListener(this);
            };
            observer.onLoadError = function(mc, error) {
                csrEI.call("csr.LoadingArtError", this.url, this.requested, String(mc), String(error));
                this.loader.removeListener(this);
            };
            this.addListener(observer);
        }
    }
    if (String(url).indexOf("img://avatar_") == 0) {
        var shower = new Object();
        shower.onLoadInit = function(mc) {
            mc._visible = true;
        };
        this.addListener(shower);
    }
    var result = csrLoadClip.apply(this, arguments);
    if (result !== true && fit != undefined) { this.removeListener(fit); }
    return result;
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
