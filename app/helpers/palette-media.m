// libpalette-media.dylib: play/pause for the bar's now-playing item (ROADMAP 20).
//
// Since macOS 15.4 MediaRemote only answers Apple-signed programs, and `osascript` can read the track but its
// commands are ignored. `/usr/bin/perl` is Apple-signed and may load this library (the owner's Widgets app does
// the same), so the bar's click runs:
//
//   PALETTE_MEDIA=toggle /usr/bin/perl -e '<loader>' /Applications/Palette.app/Contents/Frameworks/libpalette-media.dylib
//
// It sends the command to the player that's showing in Control Centre (MRNowPlayingRequest's
// localNowPlayingPlayerPath), and waits up to 2 s for MediaRemote's answer. Checked on the owner's Mac
// (2026-11-11): Spotify paused and resumed. PALETTE_MEDIA: toggle (default), play, pause, next, prev.
// Build: clang -dynamiclib -fobjc-arc -framework Foundation (app/sidecar.sh).

#import <Foundation/Foundation.h>
#include <dlfcn.h>
#include <stdlib.h>
#include <string.h>

@interface NSObject (PaletteMediaRemote)
+ (id)localNowPlayingPlayerPath;
- (id)initWithPlayerPath:(id)path;
- (void)sendCommand:(unsigned int)command options:(id)options queue:(dispatch_queue_t)queue completion:(void (^)(id))done;
@end

// Called by perl as an XSUB; its two arguments (interpreter, CV) aren't used.
void palette_media(void *interpreter, void *cv) {
    @autoreleasepool {
        // MRMediaRemoteCommand: 0 play, 1 pause, 2 toggle, 4 next track, 5 previous track.
        const char *what = getenv("PALETTE_MEDIA");
        unsigned int code = 2;
        if (what && !strcmp(what, "play")) code = 0;
        if (what && !strcmp(what, "pause")) code = 1;
        if (what && !strcmp(what, "next")) code = 4;
        if (what && !strcmp(what, "prev")) code = 5;
        if (!dlopen("/System/Library/PrivateFrameworks/MediaRemote.framework/MediaRemote", RTLD_NOW)) return;
        Class request = NSClassFromString(@"MRNowPlayingRequest");
        id path = [request localNowPlayingPlayerPath];
        if (!path) return; // nothing in Control Centre
        __block BOOL answered = NO;
        id r = [[request alloc] initWithPlayerPath:path];
        [r sendCommand:code options:@{} queue:dispatch_get_main_queue() completion:^(id result) { answered = YES; }];
        NSDate *until = [NSDate dateWithTimeIntervalSinceNow:2];
        while (!answered && [until timeIntervalSinceNow] > 0) {
            [[NSRunLoop mainRunLoop] runUntilDate:[NSDate dateWithTimeIntervalSinceNow:0.05]];
        }
    }
}
