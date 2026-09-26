#import "AccessibilityDump.h"

#import <UIKit/UIKit.h>
#import <math.h>
#import <objc/message.h>

static const NSUInteger IOSPYAXMaxNodesDefault = 2000;

static NSString *IOSPYAXString(id value) {
    if (!value || value == (id)kCFNull) {
        return nil;
    }
    if ([value isKindOfClass:[NSString class]]) {
        return value;
    }
    if ([value respondsToSelector:@selector(description)]) {
        return [value description];
    }
    return nil;
}

static NSNumber *IOSPYAXBool(BOOL value) {
    return value ? @YES : @NO;
}

static BOOL IOSPYAXResponds(id value, NSString *selectorName) {
    return value && [value respondsToSelector:NSSelectorFromString(selectorName)];
}

static BOOL IOSPYAXBoolSelector(id value, NSString *selectorName, BOOL fallback) {
    if (!IOSPYAXResponds(value, selectorName)) {
        return fallback;
    }
    SEL selector = NSSelectorFromString(selectorName);
    return ((BOOL (*)(id, SEL))objc_msgSend)(value, selector);
}

static NSArray<NSString *> *IOSPYAXTraits(UIAccessibilityTraits traits) {
    NSMutableArray<NSString *> *names = [NSMutableArray array];
    if (traits & UIAccessibilityTraitButton) [names addObject:@"button"];
    if (traits & UIAccessibilityTraitLink) [names addObject:@"link"];
    if (traits & UIAccessibilityTraitHeader) [names addObject:@"header"];
    if (traits & UIAccessibilityTraitSearchField) [names addObject:@"searchField"];
    if (traits & UIAccessibilityTraitImage) [names addObject:@"image"];
    if (traits & UIAccessibilityTraitSelected) [names addObject:@"selected"];
    if (traits & UIAccessibilityTraitPlaysSound) [names addObject:@"playsSound"];
    if (traits & UIAccessibilityTraitKeyboardKey) [names addObject:@"keyboardKey"];
    if (traits & UIAccessibilityTraitStaticText) [names addObject:@"staticText"];
    if (traits & UIAccessibilityTraitSummaryElement) [names addObject:@"summaryElement"];
    if (traits & UIAccessibilityTraitNotEnabled) [names addObject:@"notEnabled"];
    if (traits & UIAccessibilityTraitUpdatesFrequently) [names addObject:@"updatesFrequently"];
    if (traits & UIAccessibilityTraitStartsMediaSession) [names addObject:@"startsMediaSession"];
    if (traits & UIAccessibilityTraitAdjustable) [names addObject:@"adjustable"];
    if (traits & UIAccessibilityTraitAllowsDirectInteraction) [names addObject:@"allowsDirectInteraction"];
    if (traits & UIAccessibilityTraitCausesPageTurn) [names addObject:@"causesPageTurn"];
    return names;
}

static NSString *IOSPYAXRoleForElement(id element, UIAccessibilityTraits traits) {
    if (traits & UIAccessibilityTraitButton) return @"button";
    if (traits & UIAccessibilityTraitLink) return @"link";
    if (traits & UIAccessibilityTraitSearchField) return @"searchField";
    if (traits & UIAccessibilityTraitImage) return @"image";
    if (traits & UIAccessibilityTraitKeyboardKey) return @"keyboardKey";
    if (traits & UIAccessibilityTraitStaticText) return @"text";
    if ([element isKindOfClass:[UITextField class]] || [element isKindOfClass:[UITextView class]]) return @"textField";
    if ([element isKindOfClass:[UILabel class]]) return @"text";
    if ([element isKindOfClass:[UIButton class]]) return @"button";
    if ([element isKindOfClass:[UIImageView class]]) return @"image";
    if ([element isKindOfClass:[UIScrollView class]]) return @"scrollView";
    if ([element isKindOfClass:[UIWindow class]]) return @"window";
    if ([element isKindOfClass:[UIView class]]) return @"view";
    return NSStringFromClass([element class]) ?: @"element";
}

static NSDictionary *IOSPYAXFrame(id element) {
    CGRect frame = CGRectZero;
    if ([element isKindOfClass:[UIView class]]) {
        UIView *view = (UIView *)element;
        frame = [view convertRect:view.bounds toView:nil];
    } else if (IOSPYAXResponds(element, @"accessibilityFrame")) {
        frame = ((CGRect (*)(id, SEL))objc_msgSend)(element, NSSelectorFromString(@"accessibilityFrame"));
    }

    if (!isfinite(frame.origin.x) || !isfinite(frame.origin.y) ||
        !isfinite(frame.size.width) || !isfinite(frame.size.height)) {
        frame = CGRectZero;
    }
    return @{
        @"x": @(frame.origin.x),
        @"y": @(frame.origin.y),
        @"width": @(frame.size.width),
        @"height": @(frame.size.height),
    };
}

static NSArray *IOSPYAXAccessibilityChildren(id element) {
    if (!IOSPYAXResponds(element, @"accessibilityElements")) {
        return @[];
    }

    id children = ((id (*)(id, SEL))objc_msgSend)(element, NSSelectorFromString(@"accessibilityElements"));
    return [children isKindOfClass:[NSArray class]] ? children : @[];
}

static UIAccessibilityTraits IOSPYAXElementTraits(id element) {
    if (!IOSPYAXResponds(element, @"accessibilityTraits")) {
        return 0;
    }
    return ((UIAccessibilityTraits (*)(id, SEL))objc_msgSend)(element, NSSelectorFromString(@"accessibilityTraits"));
}

static NSString *IOSPYAXVisitElement(id element,
                                     NSString *parentID,
                                     NSArray<NSNumber *> *path,
                                     NSUInteger depth,
                                     NSUInteger maxDepth,
                                     NSUInteger maxNodes,
                                     BOOL includeHidden,
                                     NSMutableArray<NSDictionary *> *nodes);

static BOOL IOSPYAXElementHidden(id element) {
    if ([element isKindOfClass:[UIView class]]) {
        UIView *view = (UIView *)element;
        return view.hidden || view.alpha <= 0.01 || view.accessibilityElementsHidden;
    }
    return IOSPYAXBoolSelector(element, @"accessibilityElementsHidden", NO);
}

static void IOSPYAXVisitChild(id child,
                              NSString *nodeID,
                              NSArray<NSNumber *> *path,
                              NSUInteger depth,
                              NSUInteger maxDepth,
                              NSUInteger maxNodes,
                              BOOL includeHidden,
                              NSMutableArray<NSDictionary *> *nodes,
                              NSMutableArray<NSString *> *childIDs) {
    NSString *childID = IOSPYAXVisitElement(
        child,
        nodeID,
        path,
        depth + 1,
        maxDepth,
        maxNodes,
        includeHidden,
        nodes
    );
    if (childID) {
        [childIDs addObject:childID];
    }
}

static NSString *IOSPYAXVisitElement(id element,
                                     NSString *parentID,
                                     NSArray<NSNumber *> *path,
                                     NSUInteger depth,
                                     NSUInteger maxDepth,
                                     NSUInteger maxNodes,
                                     BOOL includeHidden,
                                     NSMutableArray<NSDictionary *> *nodes) {
    if (!element || depth > maxDepth || nodes.count >= maxNodes) {
        return nil;
    }

    BOOL hidden = IOSPYAXElementHidden(element);
    if (!includeHidden && hidden) {
        return nil;
    }

    NSString *nodeID = [NSString stringWithFormat:@"n%lu", (unsigned long)nodes.count];
    UIAccessibilityTraits traits = IOSPYAXElementTraits(element);
    BOOL focused = IOSPYAXBoolSelector(element, @"accessibilityElementIsFocused", NO);
    NSArray<NSString *> *children = @[];
    NSMutableDictionary *node = [@{
        @"id": nodeID,
        @"parent_id": parentID ?: (id)kCFNull,
        @"role": IOSPYAXRoleForElement(element, traits),
        @"label": IOSPYAXString(IOSPYAXResponds(element, @"accessibilityLabel")
            ? ((id (*)(id, SEL))objc_msgSend)(element, NSSelectorFromString(@"accessibilityLabel"))
            : nil) ?: (id)kCFNull,
        @"value": IOSPYAXString(IOSPYAXResponds(element, @"accessibilityValue")
            ? ((id (*)(id, SEL))objc_msgSend)(element, NSSelectorFromString(@"accessibilityValue"))
            : nil) ?: (id)kCFNull,
        @"hint": IOSPYAXString(IOSPYAXResponds(element, @"accessibilityHint")
            ? ((id (*)(id, SEL))objc_msgSend)(element, NSSelectorFromString(@"accessibilityHint"))
            : nil) ?: (id)kCFNull,
        @"identifier": IOSPYAXString(IOSPYAXResponds(element, @"accessibilityIdentifier")
            ? ((id (*)(id, SEL))objc_msgSend)(element, NSSelectorFromString(@"accessibilityIdentifier"))
            : nil) ?: (id)kCFNull,
        @"traits": IOSPYAXTraits(traits),
        @"enabled": IOSPYAXBool((traits & UIAccessibilityTraitNotEnabled) == 0),
        @"hidden": IOSPYAXBool(hidden),
        @"focused": IOSPYAXBool(focused),
        @"frame": IOSPYAXFrame(element),
        @"path": path,
        @"children": children,
        @"class": NSStringFromClass([element class]) ?: @"",
        @"is_accessibility_element": IOSPYAXBool(IOSPYAXBoolSelector(element, @"isAccessibilityElement", NO)),
        @"source_kind": [element isKindOfClass:[UIView class]] ? @"view" : @"accessibilityElement",
    } mutableCopy];
    [nodes addObject:node];

    NSMutableArray<NSString *> *childIDs = [NSMutableArray array];
    NSArray *accessibilityChildren = IOSPYAXAccessibilityChildren(element);
    [accessibilityChildren enumerateObjectsUsingBlock:^(id child, NSUInteger idx, BOOL *stop) {
        if (nodes.count >= maxNodes) {
            *stop = YES;
            return;
        }
        NSArray<NSNumber *> *childPath = [path arrayByAddingObjectsFromArray:@[@(0), @(idx)]];
        IOSPYAXVisitChild(
            child,
            nodeID,
            childPath,
            depth,
            maxDepth,
            maxNodes,
            includeHidden,
            nodes,
            childIDs
        );
    }];

    if ([element isKindOfClass:[UIView class]]) {
        NSArray<UIView *> *subviews = ((UIView *)element).subviews ?: @[];
        [subviews enumerateObjectsUsingBlock:^(UIView *child, NSUInteger idx, BOOL *stop) {
            if (nodes.count >= maxNodes) {
                *stop = YES;
                return;
            }
            NSArray<NSNumber *> *childPath = [path arrayByAddingObjectsFromArray:@[@(1), @(idx)]];
            IOSPYAXVisitChild(
                child,
                nodeID,
                childPath,
                depth,
                maxDepth,
                maxNodes,
                includeHidden,
                nodes,
                childIDs
            );
        }];
    }

    node[@"children"] = childIDs;
    return nodeID;
}

static NSDictionary *IOSPYAXHostApplication(void) {
    NSBundle *bundle = NSBundle.mainBundle;
    NSString *bundleID = bundle.bundleIdentifier;
    NSString *name = IOSPYAXString([bundle objectForInfoDictionaryKey:@"CFBundleDisplayName"])
        ?: IOSPYAXString([bundle objectForInfoDictionaryKey:@"CFBundleName"]);
    return @{
        @"bundle_id": bundleID ?: (id)kCFNull,
        @"name": name ?: (id)kCFNull,
        @"pid": @((NSInteger)NSProcessInfo.processInfo.processIdentifier),
    };
}

static NSArray<UIWindow *> *IOSPYAXWindows(void) {
    UIApplication *app = UIApplication.sharedApplication;
    NSMutableArray<UIWindow *> *windows = [NSMutableArray array];

    if (IOSPYAXResponds(app, @"connectedScenes")) {
        for (UIScene *scene in app.connectedScenes) {
            if (![scene isKindOfClass:[UIWindowScene class]]) {
                continue;
            }
            [windows addObjectsFromArray:((UIWindowScene *)scene).windows ?: @[]];
        }
    }

    if (windows.count == 0) {
#pragma clang diagnostic push
#pragma clang diagnostic ignored "-Wdeprecated-declarations"
        [windows addObjectsFromArray:app.windows ?: @[]];
#pragma clang diagnostic pop
    }

    return windows;
}

NSData *IOSPYAccessibilityTreeJSON(NSData *requestPayload) {
    __block NSData *result = nil;
    dispatch_sync(dispatch_get_main_queue(), ^{
        NSDictionary *request = nil;
        if (requestPayload.length > 0) {
            request = [NSJSONSerialization JSONObjectWithData:requestPayload options:0 error:nil];
        }
        NSUInteger maxDepth = [request[@"max_depth"] respondsToSelector:@selector(unsignedIntegerValue)]
            ? [request[@"max_depth"] unsignedIntegerValue]
            : 12;
        BOOL includeHidden = [request[@"include_hidden"] respondsToSelector:@selector(boolValue)]
            ? [request[@"include_hidden"] boolValue]
            : NO;
        NSUInteger maxNodes = [request[@"max_nodes"] respondsToSelector:@selector(unsignedIntegerValue)]
            ? [request[@"max_nodes"] unsignedIntegerValue]
            : IOSPYAXMaxNodesDefault;
        if (maxNodes == 0) {
            maxNodes = IOSPYAXMaxNodesDefault;
        }

        NSMutableArray<NSDictionary *> *nodes = [NSMutableArray array];
        NSArray<UIWindow *> *windows = IOSPYAXWindows();
        [windows enumerateObjectsUsingBlock:^(UIWindow *window, NSUInteger idx, BOOL *stop) {
            if (nodes.count >= maxNodes) {
                *stop = YES;
                return;
            }
            IOSPYAXVisitElement(window, nil, @[@(idx)], 0, maxDepth, maxNodes, includeHidden, nodes);
        }];

        UIScreen *screen = UIScreen.mainScreen;
        CGSize size = screen.bounds.size;
        NSDictionary *tree = @{
            @"schema": @"ioscpy.accessibility.v1",
            @"source": @"springboard-uikit-prototype",
            @"snapshot_id": [NSUUID UUID].UUIDString,
            @"timestamp_ms": @((long long)(NSDate.date.timeIntervalSince1970 * 1000.0)),
            @"focused_application": (id)kCFNull,
            @"host_application": IOSPYAXHostApplication(),
            @"screen": @{
                @"width": @(size.width),
                @"height": @(size.height),
                @"scale": @(screen.scale),
                @"orientation": @"unknown",
            },
            @"nodes": nodes,
            @"truncated": IOSPYAXBool(nodes.count >= maxNodes),
        };
        result = [NSJSONSerialization dataWithJSONObject:tree options:0 error:nil];
    });
    return result ?: [@"{\"schema\":\"ioscpy.accessibility.v1\",\"source\":\"springboard-uikit-prototype\",\"snapshot_id\":\"error\",\"nodes\":[]}" dataUsingEncoding:NSUTF8StringEncoding];
}
