import QtQuick 2.8
import QtQuick.Layouts 1.3
import QtQuick.Controls 2.3
import QtQml.Models 2.1

Pane {
    id: root

    RowLayout {
        id: tableHeader
        Layout.fillWidth: true
        width: parent.width
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
                text: qsTr("Port Range")
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
                text: qsTr("Target")
                font.bold: true
                anchors.horizontalCenter: parent.horizontalCenter
            }
        }
    }

    DelegateModel {
        property bool dragActive: false
        property int dragSrc: 0
        property int dragDst: 0
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
                drag.onActiveChanged: {
                    visualModel.dragActive = drag.active
                    if (drag.active) {
                        visualModel.dragSrc = ruleRow.DelegateModel.itemsIndex
                    } else {
                        // drag finished, update inner model
                        backend.move_rule(visualModel.dragSrc, visualModel.dragDst)
                    }
                }
            }

            DropArea {
                anchors.fill: parent

                onEntered: {
                    const s = drag.source.parent.DelegateModel.itemsIndex
                    const d = dragArea.parent.DelegateModel.itemsIndex
                    visualModel.dragDst = d
                    visualModel.items.move(s, d)
                }
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
                    currentIndex: device
                    onCurrentIndexChanged: if (device != currentIndex) device = currentIndex
                    width: firewallTitle0.width
                    model: [qsTr("Any"), qsTr("Input"), qsTr("Output")]
                }
                ComboBox {
                    x: firewallTitle1.x
                    currentIndex: proto
                    onCurrentIndexChanged: if (proto != currentIndex) proto = currentIndex
                    width: defaultFont.width * 7 + indicator.width
                    model: [qsTr("Any"), "TCP", "UDP", "UDPLite"]
                    Component.onCompleted: firewallTitle1.implicitWidth = width
                }
                TextField {
                    x: firewallTitle2.x
                    width: firewallTitle2.width
                    text: model.exe
                    onTextChanged: if (model.exe != text) model.exe = text
                }
                Control {
                    x: firewallTitle3.x
                    implicitWidth: addrIp.width + addrSlash.width + addrSubnetMask.width
                    implicitHeight: addrIp.height
                    Component.onCompleted: firewallTitle3.implicitWidth = width

                    TextField {
                        id: addrIp
                        width: defaultFont.width * 15
                        selectByMouse: true
                        text: model.addr
                        onTextChanged: if (model.addr != text) model.addr = text
                    }
                    Label {
                        id: addrSlash
                        anchors.left: addrIp.right
                        text: " / "
                        anchors.verticalCenter: parent.verticalCenter
                    }
                    TextField {
                        id: addrSubnetMask
                        anchors.left: addrSlash.right
                        width: defaultFont.width * 4
                        selectByMouse: true
                        validator: IntValidator { bottom: 0; top: 128 /*model.isV4 ? 32 : 128;*/ }
                        horizontalAlignment: TextInput.AlignHCenter
                        text: model.mask
                        onTextChanged: if (model.mask != parseInt(text)) model.mask = parseInt(text)
                    }
                }
                Control {
                    id: portRange
                    x: firewallTitle4.x
                    implicitWidth: portRangeBegin.width + portHyphen.width + portRangeEnd.width
                    implicitHeight: portRangeBegin.height
                    Component.onCompleted: firewallTitle4.implicitWidth = width

                    TextField {
                        id: portRangeBegin
                        width: font.pointSize * 5
                        validator: IntValidator{bottom: 1; top: 65535;}
                        selectByMouse: true
                        horizontalAlignment: TextInput.AlignHCenter
                        text: model.portBegin
                        onTextChanged: if (model.portBegin != parseInt(text)) model.portBegin = parseInt(text)
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
                        text: model.portEnd
                        onTextChanged: if (model.portEnd != parseInt(text)) model.portEnd = parseInt(text)
                    }
                }
                ComboBox {
                    x: firewallTitle5.x
                    currentIndex: target
                    onCurrentIndexChanged: if (target != currentIndex) target = currentIndex
                    model: defaultTarget.model
                    Component.onCompleted: firewallTitle5.implicitWidth = width
                }
                Rectangle {
                    id: removeBtn
                    property bool confirm: false
                    height: parent.height
                    width: removeBtnLabel.width + root.padding * 2
                    x: ruleRow.containsMouse && !visualModel.dragActive ?
                        (removeBtnArea.containsMouse ? root.width - root.padding - width : root.availableWidth) :
                        root.width
                    Behavior on x {
                        NumberAnimation  { duration: 200; easing.type: Easing.InOutCirc }
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
                            backend.remove_rule(ruleRow.DelegateModel.itemsIndex)
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
        anchors.top: tableHeader.bottom
        anchors.bottom: tableFooter.top
        clip: true
        width: parent.width + root.padding
        model: visualModel
        footer: Pane {
            width: parent.width - root.padding
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
        id: tableFooter
        anchors.bottom: parent.bottom
        width: parent.width
        padding: 0
        implicitHeight: applyBtn.height
        RowLayout {
            anchors.verticalCenter: parent.verticalCenter
            Label {
                text: qsTr("Default Target: ")
            }
            ComboBox {
                id: defaultTarget
                currentIndex: 0
                onCurrentIndexChanged: backend.default_target = currentIndex
                model: []
                Component.onCompleted: {
                    updateModel()
                    backend.targets_changed.connect(updateModel)
                    currentIndex = backend.default_target
                }
                function updateModel() {
                    let baseTargets = [qsTr("Accept"), qsTr("Drop")];
                    model = baseTargets.concat(backend.targets)
                }
            }
        }
        Button {
            id: applyBtn
            anchors.right: parent.right
            text: qsTr("Apply")
            onClicked: if (backend.daemon_connected) {
                backend.apply_rules()
            } else if (backend.daemon_exists()) {
                backend.connect_to_daemon()
            } else {
                startDaemonPopup.open()
            }
        }
    }
}
