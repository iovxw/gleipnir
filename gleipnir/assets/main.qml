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
                width: parent.width * 0.7
                height: parent.height * 0.7
                title: "History"
            }
            GroupBox {
                id: logs
                width: parent.width * 0.7
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
                            text: (model.input ?  "← " : "→ ") + model.protocol + " " + model.addr
                            anchors.verticalCenter: parent.verticalCenter
                        }
                        spacing: 10
                    }
                }
            }
            GroupBox {
                id: traffic
                width: parent.width * 0.3
                height: parent.height
                title: "Traffic"
                anchors.left: history.right

                ListView {
                    clip: true
                    anchors.fill: parent
                    model: ListModel {
                        ListElement {
                            exe: "/usr/bin/curl"
                            sending: 100
                            receiving: 50
                        }
                    }
                    delegate: Row {
                        Rectangle {
                            width: 40
                            height: 40
                        }

                        Label {
                            text: exe + " " + sending + " " + receiving
                            anchors.verticalCenter: parent.verticalCenter
                        }
                        spacing: 10
                    }
                }
            }
        }
    }
}
