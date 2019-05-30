import QtQuick 2.8
import QtQuick.Layouts 1.3
import QtQuick.Controls 2.3

ApplicationWindow {
    id: window
    visible: true
    //: Window title
    title: qsTr("Gleipnir")

    minimumWidth: 1000
    minimumHeight: 500

    function formatBytesRaw(bytes, decimals = 2) {
        if (bytes === 0) return [1, 'Bytes'];
        const k = 1024;
        const sizes = ['Bytes', 'KiB', 'MiB', 'GiB', 'TiB'];

        const i = Math.floor(Math.log(bytes) / Math.log(k));
        const div = Math.pow(k, i);

        return  [div, sizes[i]];
    }
    function formatBytes(bytes, decimals = 2) {
        const dm = decimals < 0 ? 0 : decimals;
        const [divisor, unit] = formatBytesRaw(bytes, decimals);
        return parseFloat((bytes / divisor).toFixed(dm)) + ' ' + unit;
    }

    TextMetrics {
        id: defaultFont
        text: "O"
    }

    TabBar {
        id: bar
        width: parent.width
        currentIndex: 0

        TabButton {
            text: qsTr("Monitor")
        }
        TabButton {
            text: qsTr("Firewall")
        }
    }

    StackLayout {
        width: parent.width
        height: parent.height - bar.height
        currentIndex: bar.currentIndex
        anchors.top: bar.bottom

        MonitorPage {}

        FirewallPage {}
    }

    Component.onCompleted: if (!backend.daemon_connected) {
        if (backend.daemon_exists()) {
            backend.connect_to_daemon()
        } else {
            startDaemonPopup.open()
        }
    }

    Popup {
        id: startDaemonPopup
        property real realY: Math.round((parent.height - height) / 2)
        parent: Overlay.overlay
        x: Math.round((parent.width - width) / 2)
        y: realY
        enter: Transition {
            NumberAnimation {
                property: "y"
                easing.type: Easing.OutBack
                from: 0
                to: startDaemonPopup.realY
                duration: 200
            }
        }
        exit: Transition {
            NumberAnimation {
                property: "y"
                easing.type: Easing.InBack
                from: startDaemonPopup.realY
                to: 0
                duration: errorPopup.visible ? 0 : 200
            }
        }
        ColumnLayout {
            anchors.fill: parent
            Label {
                text: qsTr("Daemon not found, start it manually?")
            }
            Button {
                Layout.alignment: Qt.AlignRight
                text: qsTr("Yes")
                onClicked: {
                    backend.start_daemon_error.connect((err) => {
                        errorPopup.message = qsTr("Failed to start daemon:")
                        errorPopup.error = err
                        errorPopup.open()
                    })
                    backend.start_daemon()
                    startDaemonPopup.close()
                }
            }
        }
    }
    Popup {
        id: errorPopup
        property string message: ""
        property string error: ""
        anchors.centerIn: Overlay.overlay
        enter: Transition {
            NumberAnimation {
                property: "scale"
                easing.type: Easing.OutBack
                from: 0.0
                to: 1.0
                duration: 200
            }
        }
        exit: Transition {
            NumberAnimation {
                property: "scale"
                easing.type: Easing.InBack
                from: 1.0
                to: 0.0
                duration: 200
            }
        }
        ColumnLayout {
            anchors.fill: parent
            Label {
                Layout.alignment: Qt.AlignHCenter
                text: qsTr("Error!")
                font.bold: true
            }
            MenuSeparator {
                Layout.fillWidth: true
            }
            Label {
                text: errorPopup.message
            }
            Label {
                text: errorPopup.error
                font.italic: true
                font.weight: Font.Light
            }
        }
    }
}
