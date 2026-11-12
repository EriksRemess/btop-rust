#import <Cocoa/Cocoa.h>
#import <ApplicationServices/ApplicationServices.h>
#import <Carbon/Carbon.h>
#include <signal.h>
#include <sys/stat.h>
#include <unistd.h>

// Run a separate app process and address key events and captures only to it.
static void key(pid_t pid, CGKeyCode code, CGEventFlags flags, UniChar character)
{
    ProcessSerialNumber process;
    if (GetProcessForPID(pid,&process)!=noErr) return;
    for (int down=1; down>=0; down--) {
        CGEventRef event=CGEventCreateKeyboardEvent(NULL,code,down);
        CGEventSetFlags(event,flags);
        if (character) CGEventKeyboardSetUnicodeString(event,1,&character);
        CGEventPostToPSN(&process,event); CFRelease(event);
    }
    usleep(50000);
}

static NSDictionary *visibleWindow(pid_t pid)
{
    NSArray *windows=(NSArray *)CGWindowListCopyWindowInfo(kCGWindowListOptionOnScreenOnly,kCGNullWindowID);
    NSDictionary *largest=nil; double area=0;
    for (NSDictionary *window in windows) {
        if ([[window objectForKey:(id)kCGWindowOwnerPID] intValue]!=pid ||
            [[window objectForKey:(id)kCGWindowLayer] intValue]!=0) continue;
        CGRect bounds;
        CGRectMakeWithDictionaryRepresentation((CFDictionaryRef)[window objectForKey:(id)kCGWindowBounds],&bounds);
        double candidate=bounds.size.width*bounds.size.height;
        if (candidate>area) { area=candidate; largest=window; }
    }
    [[largest retain] autorelease]; [windows release]; return largest;
}

static BOOL fullscreen(NSDictionary *window)
{
    if (!window) return NO;
    CGRect bounds;
    CGRectMakeWithDictionaryRepresentation((CFDictionaryRef)[window objectForKey:(id)kCGWindowBounds],&bounds);
    CGSize screen=CGDisplayBounds(CGMainDisplayID()).size;
    return CGSizeEqualToSize(bounds.size,screen);
}

static BOOL capture(NSDictionary *window, NSString *path)
{
    if (!window) return NO;
    CGWindowID number=[[window objectForKey:(id)kCGWindowNumber] unsignedIntValue];
    CGImageRef image=CGWindowListCreateImage(CGRectNull,kCGWindowListOptionIncludingWindow,number,kCGWindowImageBoundsIgnoreFraming);
    if (!image) return NO;
    NSBitmapImageRep *bitmap=[[NSBitmapImageRep alloc] initWithCGImage:image];
    NSData *png=[bitmap representationUsingType:NSPNGFileType properties:[NSDictionary dictionary]];
    BOOL ok=[png writeToFile:path atomically:YES];
    [bitmap release]; CGImageRelease(image); return ok;
}


static NSString *quote(NSString *value) {
    return [NSString stringWithFormat:@"'%@'",[value stringByReplacingOccurrencesOfString:@"'" withString:@"'\\''"]];
}
int main(int argc,char **argv) {
    if(argc!=4) {fprintf(stderr,"Usage: ui-probe SNOW_GHOSTTY MONITOR TEST_DIRECTORY\n");return 2;}
    NSAutoreleasePool *pool=[[NSAutoreleasePool alloc] init];
    NSString *root=[NSString stringWithUTF8String:argv[3]];
    NSString *command=[NSString stringWithFormat:@"exec /opt/local/bin/python3.13 %@ %@ %@ %@",
        quote([root stringByAppendingPathComponent:@"run-monitor.py"]),quote([NSString stringWithUTF8String:argv[2]]),
        quote([root stringByAppendingPathComponent:@"btop.conf"]),quote([root stringByAppendingPathComponent:@"result.json"])];
    NSTask *task=[[NSTask alloc] init];[task setLaunchPath:[NSString stringWithUTF8String:argv[1]]];
    [task setArguments:[NSArray arrayWithObjects:@"--config",[root stringByAppendingPathComponent:@"snow.conf"],@"--command",command,nil]];
    NSMutableDictionary *env=[NSMutableDictionary dictionaryWithDictionary:[[NSProcessInfo processInfo] environment]];
    [env setObject:root forKey:@"HOME"];[env setObject:[root stringByAppendingPathComponent:@"config"] forKey:@"XDG_CONFIG_HOME"];
    [env setObject:@"en_US.UTF-8" forKey:@"LANG"];[task setEnvironment:env];
    [task launch];pid_t pid=[task processIdentifier];
    NSDictionary *window=nil;
    for(int n=0;n<100 && [task isRunning];n++){window=visibleWindow(pid);if(window)break;usleep(100000);}
    BOOL ok=window!=nil;usleep(4000000);
    if(ok)ok=capture(visibleWindow(pid),[root stringByAppendingPathComponent:@"monitor.png"]);
    key(pid,31,0,'o');usleep(500000);if(ok)ok=capture(visibleWindow(pid),[root stringByAppendingPathComponent:@"options.png"]);
    key(pid,53,0,27);key(pid,4,0,'h');usleep(500000);if(ok)ok=capture(visibleWindow(pid),[root stringByAppendingPathComponent:@"help.png"]);
    key(pid,53,0,27);
    key(pid,36,kCGEventFlagMaskCommand,0);usleep(1500000);window=visibleWindow(pid);ok=ok && fullscreen(window);
    if(ok)ok=capture(window,[root stringByAppendingPathComponent:@"fullscreen.png"]);
    key(pid,36,kCGEventFlagMaskCommand,0);usleep(1000000);window=visibleWindow(pid);ok=ok && window && !fullscreen(window);
    key(pid,3,0,'f');const UniChar filter[]={'b','t','o','p','r','s'};
    for(unsigned n=0;n<sizeof(filter)/sizeof(filter[0]);n++)key(pid,0,0,filter[n]);
    key(pid,36,0,'\r');usleep(500000);
    if(ok)ok=capture(visibleWindow(pid),[root stringByAppendingPathComponent:@"filtered.png"]);
    key(pid,12,0,'q');
    for(int n=0;n<100;n++){if(!visibleWindow(pid))break;usleep(100000);}
    ok=ok && visibleWindow(pid)==nil;
    if([task isRunning]) {
        key(pid,12,kCGEventFlagMaskCommand,0);
        for(int n=0;n<30 && [task isRunning];n++)usleep(100000);
        if([task isRunning]){[task terminate];[task waitUntilExit];}
    }
    fprintf(stderr,"Snow Ghostty monitor/options/help/filter/fullscreen/restore/exit: %s\n",ok?"PASS":"FAIL");
    [task release];[pool drain];return ok?0:1;
}
