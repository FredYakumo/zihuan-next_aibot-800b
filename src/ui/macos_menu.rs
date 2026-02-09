#![cfg(target_os = "macos")]
#![allow(unexpected_cfgs)]

use cocoa::appkit::{NSApp, NSMenu, NSMenuItem};
use cocoa::base::{id, nil};
use cocoa::foundation::{NSAutoreleasePool, NSString};
use objc::declare::ClassDecl;
use objc::runtime::{Class, Object, Sel};
use objc::{class, msg_send, sel, sel_impl};
use std::os::raw::c_void;
use std::sync::Once;

pub struct MenuActions {
    pub open: Box<dyn Fn() + 'static>,
    pub save: Box<dyn Fn() + 'static>,
    pub new_tab: Box<dyn Fn() + 'static>,
    pub close_tab: Box<dyn Fn() + 'static>,
    pub quit: Box<dyn Fn() + 'static>,
}

fn menu_handler_class() -> *const Class {
    static INIT: Once = Once::new();
    static mut CLASS: *const Class = std::ptr::null();

    unsafe {
        INIT.call_once(|| {
            let superclass = class!(NSObject);
            let mut decl = ClassDecl::new("ZihuanMenuHandler", superclass)
                .expect("Failed to create ZihuanMenuHandler class");
            decl.add_ivar::<*mut c_void>("rustActions");
            decl.add_method(sel!(openJson:), open_json as extern "C" fn(&Object, Sel, id));
            decl.add_method(sel!(saveJson:), save_json as extern "C" fn(&Object, Sel, id));
            decl.add_method(sel!(newTab:), new_tab as extern "C" fn(&Object, Sel, id));
            decl.add_method(sel!(closeTab:), close_tab as extern "C" fn(&Object, Sel, id));
            decl.add_method(sel!(quit:), quit as extern "C" fn(&Object, Sel, id));
            CLASS = decl.register();
        });
        CLASS
    }
}

unsafe fn with_actions<F: FnOnce(&MenuActions)>(this: &Object, f: F) {
    let ptr: *mut c_void = *this.get_ivar("rustActions");
    if ptr.is_null() {
        return;
    }
    let actions = &*(ptr as *mut MenuActions);
    f(actions);
}

extern "C" fn open_json(this: &Object, _: Sel, _: id) {
    unsafe { with_actions(this, |actions| (actions.open)()); }
}

extern "C" fn save_json(this: &Object, _: Sel, _: id) {
    unsafe { with_actions(this, |actions| (actions.save)()); }
}

extern "C" fn new_tab(this: &Object, _: Sel, _: id) {
    unsafe { with_actions(this, |actions| (actions.new_tab)()); }
}

extern "C" fn close_tab(this: &Object, _: Sel, _: id) {
    unsafe { with_actions(this, |actions| (actions.close_tab)()); }
}

extern "C" fn quit(this: &Object, _: Sel, _: id) {
    unsafe {
        with_actions(this, |actions| (actions.quit)());
        let app = NSApp();
        let _: () = msg_send![app, terminate: nil];
    }
}

unsafe fn create_handler(actions: MenuActions) -> id {
    let cls = menu_handler_class();
    let handler: id = msg_send![cls, new];
    let boxed = Box::new(actions);
    let ptr = Box::into_raw(boxed) as *mut c_void;
    (*handler).set_ivar("rustActions", ptr);
    handler
}

unsafe fn menu_item(title: &str, action: Sel, key: &str, target: id) -> id {
    let ns_title = NSString::alloc(nil).init_str(title);
    let ns_key = NSString::alloc(nil).init_str(key);
    let item: id = msg_send![class!(NSMenuItem), alloc];
    let item: id = msg_send![item, initWithTitle: ns_title action: action keyEquivalent: ns_key];
    let _: () = msg_send![item, setTarget: target];
    item
}

pub fn install_menu(actions: MenuActions) {
    unsafe {
        let _pool = NSAutoreleasePool::new(nil);
        let app = NSApp();
        let handler = create_handler(actions);

        // Get the existing menu bar created by winit/Slint, and append our menus to it
        let menubar: id = msg_send![app, mainMenu];
        if menubar == nil {
            eprintln!("[macOS menu] mainMenu is nil, cannot install menu");
            return;
        }

        let count: i64 = msg_send![menubar, numberOfItems];

        // --- "文件" menu ---
        let file_menu_title = NSString::alloc(nil).init_str("文件");
        let file_menu = NSMenu::alloc(nil).initWithTitle_(file_menu_title);

        file_menu.addItem_(menu_item("打开…", sel!(openJson:), "o", handler));
        file_menu.addItem_(menu_item("保存", sel!(saveJson:), "s", handler));
        let separator: id = msg_send![class!(NSMenuItem), separatorItem];
        file_menu.addItem_(separator);
        file_menu.addItem_(menu_item("新建节点图", sel!(newTab:), "t", handler));
        file_menu.addItem_(menu_item("关闭节点图", sel!(closeTab:), "w", handler));

        let file_menu_item = NSMenuItem::new(nil).autorelease();
        let _: () = msg_send![file_menu_item, setTitle: file_menu_title];
        file_menu_item.setSubmenu_(file_menu);

        // Insert after the app menu (index 1), or append if only app menu exists
        if count >= 1 {
            let _: () = msg_send![menubar, insertItem: file_menu_item atIndex: 1i64];
        } else {
            menubar.addItem_(file_menu_item);
        }

        let new_count: i64 = msg_send![menubar, numberOfItems];
    }
}
