// palette verify: what Rex took, read from its preferences (read-only). Rex keeps its custom
// themes as JSON inside a binary preference, which `defaults read` prints cut short.
ObjC.import('Foundation');
const d = $.NSUserDefaults.alloc.initWithSuiteName('com.superlogical.rex');
const s = k => { const v = d.stringForKey(k); return v.isNil() ? '' : v.js; };
const raw = d.objectForKey('Workspace.customThemes');
let json = '[]';
if (!raw.isNil()) {
  json = raw.isKindOfClass($.NSData) ? $.NSString.alloc.initWithDataEncoding(raw, $.NSUTF8StringEncoding).js : ObjC.unwrap(raw);
}
const hex = c => c ? [c.red, c.green, c.blue].map(x => Math.round(x * 255).toString(16).padStart(2, '0')).join('') : '';
const t = JSON.parse(json).find(x => x.name === 'palette') || {};
['dark=' + s('Workspace.darkThemeID'), 'light=' + s('Workspace.lightThemeID'), 'appearance=' + s('Workspace.appearance'),
 'id=' + (t.id || ''), 'filed=' + (t.appearance || ''), 'bg=' + hex(t.palette && t.palette.background)].join('\n');
