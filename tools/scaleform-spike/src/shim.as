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
    var o;
    if (parts[0] == "_global") {
        o = _global;
    } else {
        o = eval(parts[0]);
    }
    var i = 1;
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
