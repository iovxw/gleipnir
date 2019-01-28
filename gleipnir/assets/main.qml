import QtQuick 2.8
import QtQuick.Layouts 1.3
import QtQuick.Controls 2.3
import QtGraphicalEffects 1.0
import Qt.labs.platform 1.0

ApplicationWindow {
    id: window
    visible: true
    //: Window title
    title: qsTr("Gleipnir")

    width: 900
    height: 500
    minimumWidth: 640
    minimumHeight: 480

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

        Item {
            GroupBox {
                id: history
                width: parent.width - traffic.width
                height: parent.height * 0.7
                title: "History"
            }
            GroupBox {
                id: logs
                width: parent.width - traffic.width
                height: parent.height * 0.3
                title: "Logs"
                anchors.top: history.bottom

                ListView {
                    clip: true
                    anchors.fill: parent
                    model: ListModel {
                        ListElement {
                            dropped: false
                            input: true
                            protocol: "TCP"
                            addr: "1.1.1.1:443"
                            len: 42
                            matched_rule: 1
                        }
                        ListElement {
                            dropped: true
                            input: false
                            protocol: "TCP"
                            addr: "1.1.1.1:443"
                            len: 42
                            matched_rule: 2
                        }
                        ListElement {
                            dropped: false
                            input: true
                            protocol: "TCP"
                            addr: "1.1.1.1:443"
                            len: 42
                            matched_rule: 1
                        }
                    }
                    delegate: Row {
                        Rectangle {
                            width: 40
                            height: 40
                            color: if (model.dropped) { "red" } else { "green" }
                        }

                        Label {
                            text: (model.input ?  "⇤" : "↦") + model.protocol + " " + model.addr
                            anchors.verticalCenter: parent.verticalCenter
                        }
                        spacing: 10
                    }
                }
            }
            Frame{
                id: traffic
                width: Math.max(parent.width * 0.3, 300)
                height: parent.height
                anchors.left: history.right
                topPadding: 0
                ColumnLayout {
                    anchors.fill: parent

                    RowLayout {
                        Layout.fillWidth: true
                        height: separator.implicitHeight
                        spacing: 0

                        Pane {
                            id: historyTitle0
                            topPadding: 0
                            bottomPadding: 0
                            Layout.fillWidth: true
                            Label {
                                text: "Program"
                                font.bold: true
                                anchors.horizontalCenter: parent.horizontalCenter
                            }
                        }
                        ToolSeparator {
                            id: separator
                        }
                        Pane {
                            id: historyTitle1
                            implicitWidth: defaultFont.width * 8
                            padding: 0
                            Label {
                                text: "↑"
                                font.bold: true
                                anchors.horizontalCenter: parent.horizontalCenter
                            }
                        }
                        ToolSeparator {}
                        Pane {
                            id: historyTitle2
                            implicitWidth: defaultFont.width * 8
                            padding: 0
                            Label {
                                text: "↓"
                                font.bold: true
                                anchors.horizontalCenter: parent.horizontalCenter
                            }
                        }
                    }

                    ListView {
                        clip: true
                        Layout.fillHeight: true
                        Layout.fillWidth: true
                        model: ListModel {
                            ListElement {
                                exe: "/usr/bin/curl"
                                sending: 1000000
                                receiving: 90000000
                            }
                            ListElement {
                                exe: "/usr/bin/curl"
                                sending: 123
                                receiving: 500000
                            }
                        }
                        delegate: Item {
                            width: parent.width
                            height: separator.implicitHeight

                            Label {
                                clip: true
                                width: historyTitle0.width
                                text: model.exe
                            }
                            Label {
                                x: historyTitle1.x + historyTitle1.width - width
                                text: formatBytes(model.sending) + "/s"
                            }
                            Label {
                                anchors.right: parent.right
                                text: formatBytes(model.receiving) + "/s"
                            }
                        }
                    }
                }
            }
        }
    }
}
