#![feature(try_trait)]
#![feature(try_blocks)]
#![feature(result_map_or_else)]
#![feature(vec_remove_item)]
#![feature(async_await)]
#![recursion_limit = "128"]

#[global_allocator]
static ALLOC: jemallocator::Jemalloc = jemallocator::Jemalloc;

use cpp::*;
use qmetaobject::*;

mod implementation;
mod listmodel;
mod monitor;

cpp! {{
    #include <memory>
    #include <QtGui/QIcon>
    #include <QtQuick/QtQuick>
    #include <QtCore/QTranslator>
    #include <QtWidgets/QApplication>

    static QTranslator translator;

    struct QmlEngineHolder {
        std::unique_ptr<QApplication> app;
        std::unique_ptr<QQmlApplicationEngine> engine;
        std::unique_ptr<QQuickView> view;
    };
}}

#[cfg(not(debug_assertions))]
qrc! { init_ressource,
     "/" {
         "assets/main.qml",
         "assets/MonitorPage.qml",
         "assets/FirewallPage.qml",
         "assets/RateLimitRulesPopup.qml",
         "assets/i18n/zh_CN.qm",
     },
}

fn main() {
    #[cfg(not(debug_assertions))]
    init_ressource();

    unsafe {
        cpp!([] {
            QCoreApplication::setAttribute(Qt::AA_EnableHighDpiScaling);
        })
    }

    let mut engine = QmlEngine::new();

    let backend = implementation::Backend::new();
    let backend = QObjectBox::new(backend);
    let backend = backend.pinned();

    engine.set_object_property("backend".into(), backend);

    let engine = &mut engine;
    unsafe {
        cpp!([engine as "QmlEngineHolder*"] {
            translator.load(QLocale::system(), "", "", ":/assets/i18n");
            QApplication::installTranslator(&translator);

            engine->app->setOrganizationName("iovxw");
            engine->app->setOrganizationDomain("iovxw.net");
            engine->app->setApplicationName("Gleipnir");

            //auto icon = QIcon::fromTheme("x", QIcon(":/assets/x.svg"));
            //engine->app->setWindowIcon(icon);
        });
    }
    engine.load_file(
        #[cfg(debug_assertions)]
        concat!(env!("CARGO_MANIFEST_DIR"), "/assets/main.qml").into(),
        #[cfg(not(debug_assertions))]
        "qrc:/assets/main.qml".into(),
    );

    engine.exec();
}
