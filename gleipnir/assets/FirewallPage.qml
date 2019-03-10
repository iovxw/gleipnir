import QtQuick 2.8
import QtQuick.Layouts 1.3
import QtQuick.Controls 2.3
import QtQml.Models 2.1

Pane {
    ColumnLayout {
        anchors.fill: parent

        RowLayout {
            Layout.fillWidth: true
            height: separator.implicitHeight
            spacing: 0

            Pane {
                id: firewallTitle0
                topPadding: 0
                bottomPadding: 0
                Label {
                    text: qsTr("Direction")
                    font.bold: true
                    anchors.horizontalCenter: parent.horizontalCenter
                }
            }
            ToolSeparator {
                id: separator
            }
            Pane {
                id: firewallTitle1
                topPadding: 0
                bottomPadding: 0
                Label {
                    text: qsTr("Proto")
                    font.bold: true
                    anchors.horizontalCenter: parent.horizontalCenter
                }
            }
            ToolSeparator {}
            Pane {
                id: firewallTitle2
                topPadding: 0
                bottomPadding: 0
                Layout.fillWidth: true
                Label {
                    text: qsTr("Program")
                    font.bold: true
                    anchors.horizontalCenter: parent.horizontalCenter
                }
            }
            ToolSeparator {}
            Pane {
                id: firewallTitle3
                topPadding: 0
                bottomPadding: 0
                implicitWidth: defaultFont.width * 15
                Label {
                    text: qsTr("Address")
                    font.bold: true
                    anchors.horizontalCenter: parent.horizontalCenter
                }
            }
            ToolSeparator {}
            Pane {
                id: firewallTitle4
                topPadding: 0
                bottomPadding: 0
                Label {
                    text: qsTr("Subnet Mask")
                    font.bold: true
                    anchors.horizontalCenter: parent.horizontalCenter
                }
            }
            ToolSeparator {}
            Pane {
                id: firewallTitle5
                topPadding: 0
                bottomPadding: 0
                Label {
                    text: qsTr("Port Range")
                    font.bold: true
                    anchors.horizontalCenter: parent.horizontalCenter
                }
            }
            ToolSeparator {}
            Pane {
                id: firewallTitle6
                topPadding: 0
                bottomPadding: 0
                Label {
                    text: qsTr("Target")
                    font.bold: true
                    anchors.horizontalCenter: parent.horizontalCenter
                }
            }
        }

        DelegateModel {
            property bool dragActive: false
            id: visualModel
            model: backend.rules
            delegate: MouseArea {
                id: ruleRow
                hoverEnabled: true
                height: content.height
                width: parent.width

                MouseArea {
                    id: dragArea

                    anchors.fill: parent

                    drag.target: content
                    drag.axis: Drag.YAxis
                    drag.onActiveChanged: visualModel.dragActive = drag.active
                }

                DropArea {
                    anchors.fill: parent

                    onEntered: visualModel.items.move(
                        drag.source.parent.DelegateModel.itemsIndex,
                        dragArea.parent.DelegateModel.itemsIndex,
                    )
                }

                Pane {
                    id: content

                    // Used to fix position when drag finished
                    anchors.verticalCenter: parent.verticalCenter
                    padding: 0
                    topPadding: separator.padding
                    bottomPadding: topPadding
                    implicitHeight: direction.height + topPadding + bottomPadding
                    width: 0 // Do not cover the dragArea

                    Drag.active: dragArea.pressed
                    Drag.source: dragArea
                    Drag.hotSpot.x: width / 2
                    Drag.hotSpot.y: height / 2
                    states: State {
                        when: dragArea.drag.active

                        ParentChange { target: content; parent: rulesTable }
                        AnchorChanges {
                            target: content
                            // Free the anchors when dragging
                            anchors.verticalCenter: undefined
                        }
                    }

                    ComboBox {
                        id: direction
                        x: firewallTitle0.x
                        currentIndex: 0
                        onCurrentIndexChanged: device = currentIndex
                        width: firewallTitle0.width
                        model: [qsTr("Any"), qsTr("Input"), qsTr("Output")]
                        Component.onCompleted: currentIndex = device
                    }
                    ComboBox {
                        x: firewallTitle1.x
                        currentIndex: 0
                        onCurrentIndexChanged: proto = currentIndex
                        width: defaultFont.width * 7 + indicator.width
                        model: [qsTr("Any"), "TCP", "UDP", "UDPLite"]
                        Component.onCompleted: {
                            firewallTitle1.implicitWidth = width
                            currentIndex = proto
                        }
                    }
                    TextField {
                        x: firewallTitle2.x
                        width: firewallTitle2.width
                        text: ""
                        onTextChanged: model.exe = text
                        Component.onCompleted: text = model.exe
                    }
                    TextField {
                        x: firewallTitle3.x
                        width: firewallTitle3.width
                        selectByMouse: true
                        text: ""
                        onTextChanged: model.addr = text
                        Component.onCompleted: text = model.addr
                    }
                    TextField {
                        x: firewallTitle4.x + (firewallTitle4.width - width) / 2
                        width: defaultFont.width * 4
                        selectByMouse: true
                        validator: IntValidator { bottom: 0; top: 128 /*model.isV4 ? 32 : 128;*/ }
                        horizontalAlignment: TextInput.AlignHCenter
                        text: ""
                        onTextChanged: model.mask = parseInt(text)
                        Component.onCompleted: text = model.mask
                    }
                    Control {
                        id: portRange
                        x: firewallTitle5.x
                        implicitWidth: portRangeBegin.width + portHyphen.width + portRangeEnd.width
                        implicitHeight: portRangeBegin.height
                        Component.onCompleted: firewallTitle5.implicitWidth = width

                        Timer {
                            id: portChecker
                            interval: 1000
                            onTriggered: {
                                let portS = parseInt(portRangeBegin.text)
                                let portE = parseInt(portRangeEnd.text)
                                if (portS == 0) {
                                    portRangeBegin.text = ""
                                } else if (portS > 65536) {
                                    portRangeBegin.text = 65535
                                }
                                if (portE == 0) {
                                    portRangeEnd.text = ""
                                } else if (portE > 65536) {
                                    portRangeEnd.text = 65535
                                }
                                if (portS > portE) {
                                    portRangeEnd.text = portRangeBegin.text
                                }
                            }
                        }
                        TextField {
                            id: portRangeBegin
                            width: font.pointSize * 5
                            validator: IntValidator{bottom: 1; top: 65535;}
                            selectByMouse: true
                            horizontalAlignment: TextInput.AlignHCenter
                            text: ""
                            onTextChanged: model.portBegin = parseInt(text)
                            onTextEdited: {
                                portChecker.stop()
                                portChecker.start()
                            }
                            Component.onCompleted: text = model.portBegin
                        }
                        Label {
                            id: portHyphen
                            anchors.left: portRangeBegin.right
                            text: " - "
                            anchors.verticalCenter: parent.verticalCenter
                        }
                        TextField {
                            id: portRangeEnd
                            anchors.left: portHyphen.right
                            width: font.pointSize * 5
                            validator: IntValidator{bottom: 1; top: 65535;}
                            selectByMouse: true
                            horizontalAlignment: TextInput.AlignHCenter
                            text: ""
                            onTextChanged: model.portEnd = parseInt(text)
                            onTextEdited: {
                                portChecker.stop()
                                portChecker.start()
                            }
                            Component.onCompleted: text = model.portEnd
                        }
                    }
                    ComboBox {
                        x: firewallTitle6.x
                        currentIndex: 0
                        onCurrentIndexChanged: target = currentIndex
                        model: []
                        Component.onCompleted: {
                            firewallTitle6.implicitWidth = width
                            updateModel()
                            backend.targets_changed.connect(updateModel)
                            currentIndex = target
                        }
                        function updateModel() {
                            if (model.length && !backend.targets.includes(model[currentIndex])) {
                                // Old target is removed, reset selected target
                                currentIndex = 0
                            }
                            let baseTargets = [qsTr("Accept"), qsTr("Drop")];
                            model = baseTargets.concat(backend.targets)
                        }
                    }
                    Rectangle {
                        id: removeBtn
                        property bool confirm: false
                        height: content.implicitHeight - content.topPadding - content.bottomPadding
                        width: removeBtnLabel.width + 20
                        x: ruleRow.containsMouse && !visualModel.dragActive ?
                            (removeBtnArea.containsMouse ? ruleRow.width - width : ruleRow.width - 10) :
                            ruleRow.width
                        Behavior on x {
                            NumberAnimation  { duration: 200; easing.type: Easing.InOutCubic }
                        }
                        color: confirm ? "green" : "red"
                        Label {
                            id: removeBtnLabel
                            anchors.centerIn: parent
                            text: parent.confirm ? qsTr("Confirm") : qsTr("Remove")
                            color: "white"
                            font.bold: true
                        }
                        MouseArea {
                            id: removeBtnArea
                            anchors.fill: parent
                            hoverEnabled: true
                            onClicked: if (parent.confirm) {
                                visualModel.items.remove(ruleRow.DelegateModel.itemsIndex)
                            } else {
                                parent.confirm = true
                            }
                            onExited: parent.confirm = false
                        }
                    }
                }
            }
        }

        ListView {
            id: rulesTable
            clip: true
            Layout.fillHeight: true
            Layout.fillWidth: true
            model: visualModel
            footer: Pane {
                width: parent.width
                padding: 0
                topPadding: separator.padding
                bottomPadding: topPadding

                Button {
                    id: addBtn
                    width: parent.width
                    text: "+"
                    scale: !visualModel.dragActive ? 1.0 : 0.0
                    Behavior on scale {
                        NumberAnimation  { duration: 250; easing.type: Easing.InOutCubic }
                    }
                    onClicked: backend.new_rule()
                }
            }
        }

        Pane {
            Layout.fillWidth: true
            padding: 0
            Button {
                anchors.right: parent.right
                text: qsTr("Apply")
                onClicked: backend.apply_rules()
            }
        }
    }
}
