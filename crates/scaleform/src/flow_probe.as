// Explicit read-only public display-list observation, invoked only by the
// opt-in rendered acceptance harness. No timers, observers or movie setters.
_global.csrDiagnostics.flowProbe = function(id, paths, searchRoot, needle) {
    var result = new Object();
    var i = 0;
    while (i < paths.length) {
        var clip = _global.csrResolve(paths[i]);
        var visible = typeof clip == "movieclip";
        var ancestor = clip;
        var guard = 0;
        while (visible && ancestor != undefined && guard < 32) {
            visible = ancestor._visible != false && ancestor._alpha > 0;
            ancestor = ancestor._parent;
            guard += 1;
        }
        result["visible" + i] = visible;
        i += 1;
    }
    result.stageWidth = Stage.width;
    result.stageHeight = Stage.height;
    var budget = 1024;
    var matches = 0;
    var labelCount = 0;
    var visited = [];
    var visit = function(o, depth) {
        if (o == undefined || o._visible == false || o._alpha <= 0) { return; }
        var index = 0;
        while (index < visited.length) {
            if (visited[index] == o) { return; }
            index += 1;
        }
        visited.push(o);
        budget -= 1;
        if (depth > 12) { budget = -1; }
        if (budget < 0) { return; }
        if (typeof o.text == "string") {
            var text = o.text.toUpperCase().split("\r").join("").split("\n").join("");
            if (labelCount < 24 && text.length > 0) {
                result["label" + labelCount] = text.substr(0, 96);
                labelCount += 1;
            }
            // Caller supplies the original English menu label. Ambiguity is a
            // failed observation, never a reason to choose a guessed hit box.
            if (text == needle) {
                var a = {x:o._x, y:o._y};
                var b = {x:o._x + o._width, y:o._y + o._height};
                o._parent.localToGlobal(a);
                o._parent.localToGlobal(b);
                result.x = (a.x + b.x) / 2;
                result.y = (a.y + b.y) / 2;
                result.path = String(o);
                matches += 1;
            }
            return;
        }
        for (var key in o) {
            var child = o[key];
            if ((typeof child == "movieclip" || typeof child == "object") && child._parent == o) {
                visit(child, depth + 1);
            }
        }
    };
    if (needle != "") { visit(_global.csrResolve(searchRoot), 0); }
    result.matches = matches;
    result.complete = budget >= 0;
    // Retain only the last sample, addressed by request identity. The host's
    // existing Inspect message reads it after this call on the same queue.
    _global.csrFlowProbe = new Object();
    _global.csrFlowProbe[id] = result;
};
