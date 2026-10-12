ObjC.import('CoreGraphics');
function run(argv) {
  const pid = +argv[0];
  const list = ObjC.castRefToObject($.CGWindowListCopyWindowInfo(1, 0));
  const out = [];
  for (let i = 0; i < list.count; i++) {
    const w = list.objectAtIndex(i);
    if (pid && w.objectForKey('kCGWindowOwnerPID').intValue != pid) continue;
    const b = w.objectForKey('kCGWindowBounds');
    out.push([w.objectForKey('kCGWindowLayer').intValue, b.objectForKey('X').intValue, b.objectForKey('Y').intValue, b.objectForKey('Width').intValue, b.objectForKey('Height').intValue].join(','));
  }
  return out.join(';');
}
