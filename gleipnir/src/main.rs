#![feature(type_alias_enum_variants)]
#![feature(try_trait)]
#![feature(try_blocks)]
#![feature(result_map_or_else)]
#![feature(vec_remove_item)]
#![recursion_limit = "128"]

use std::fs::{self, File};
use std::io::prelude::*;
use std::os::unix::fs::PermissionsExt;

use cpp::*;
use qmetaobject::*;

mod implementation;
mod listmodel;

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

qrc! { init_ressource,
     "/" {
         "assets/main.qml",
         "assets/MonitorPage.qml"
         "assets/FirewallPage.qml"
     },
}

fn main() {
    init_ressource();

    let mut engine = QmlEngine::new();

    let engine = &mut engine;
    unsafe {
        cpp!([engine as "QmlEngineHolder*"] {
            QCoreApplication::setAttribute(Qt::AA_EnableHighDpiScaling);

            translator.load(QLocale::system(), "", "", ":/assets/i18n");
            QApplication::installTranslator(&translator);

            //auto icon = QIcon::fromTheme("x", QIcon(":/assets/x.svg"));
            //engine->app->setWindowIcon(icon);
        });
    }
    engine.load_file("qrc:/assets/main.qml".into());
    engine.exec();
}
