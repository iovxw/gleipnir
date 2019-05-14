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

    function formatBytes(bytes, decimals = 2) {
        if (bytes === 0) return '0 Bytes';

        const k = 1024;
        const dm = decimals < 0 ? 0 : decimals;
        const sizes = ['Bytes', 'KiB', 'MiB', 'GiB', 'TiB'];

        const i = Math.floor(Math.log(bytes) / Math.log(k));

        return parseFloat((bytes / Math.pow(k, i)).toFixed(dm)) + ' ' + sizes[i];
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

    Popup {
        id: startDaemonPopup
        anchors.centerIn: Overlay.overlay
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
