import QtQuick 2.8
import QtQuick.Layouts 1.3
import QtQuick.Controls 2.3
import QtQuick.Dialogs 1.3
import QtQml.Models 2.1

Pane {
    id: root

    function parseIntDefault(string) {
        const n = parseInt(string)
        return n ? n : 0
    }
    function parsePort(string) {
        const port = parseIntDefault(string);
        return port > 25565 ? 25565 : port;
    }

    function isIP(s) {
        // IPv4 Segment
        const v4Seg = '(?:[0-9]|[1-9][0-9]|1[0-9][0-9]|2[0-4][0-9]|25[0-5])';
        const v4Str = `(${v4Seg}[.]){3}${v4Seg}`;
        const IPv4Reg = new RegExp(`^${v4Str}$`);

        // IPv6 Segment
        const v6Seg = '(?:[0-9a-fA-F]{1,4})';
        const IPv6Reg = new RegExp('^(' +
                                   `(?:${v6Seg}:){7}(?:${v6Seg}|:)|` +
                                   `(?:${v6Seg}:){6}(?:${v4Str}|:${v6Seg}|:)|` +
                                   `(?:${v6Seg}:){5}(?::${v4Str}|(:${v6Seg}){1,2}|:)|` +
                                   `(?:${v6Seg}:){4}(?:(:${v6Seg}){0,1}:${v4Str}|(:${v6Seg}){1,3}|:)|` +
                                   `(?:${v6Seg}:){3}(?:(:${v6Seg}){0,2}:${v4Str}|(:${v6Seg}){1,4}|:)|` +
                                   `(?:${v6Seg}:){2}(?:(:${v6Seg}){0,3}:${v4Str}|(:${v6Seg}){1,5}|:)|` +
                                   `(?:${v6Seg}:){1}(?:(:${v6Seg}){0,4}:${v4Str}|(:${v6Seg}){1,6}|:)|` +
                                   `(?::((?::${v6Seg}){0,5}:${v4Str}|(?::${v6Seg}){1,7}|:))` +
                                   ')(%[0-9a-zA-Z]{1,})?$');

        if (IPv4Reg.test(s)) return 4;
        if (IPv6Reg.test(s)) return 6;
        return 0;
    }

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
                RowLayout {
                    x: firewallTitle2.x
                    width: firewallTitle2.width
                    TextField {
                        Layout.fillWidth: true
                        text: model.exe
                        onTextChanged: if (model.exe != text) model.exe = text
                        selectByMouse: true
                    }
                    RoundButton {
                        text: "..."
                        onClicked: fileDialog.open()
                    }
                    FileDialog {
                        id: fileDialog
                        title: qsTr("Please choose a program")
                        folder: "/usr/bin"
                        onAccepted: {
                            let path = fileDialog.fileUrl.toString()
                            path = path.replace(/^(file:\/{2})/,"")
                            path = decodeURIComponent(path)
                            model.exe = path
                        }
                    }
                }
                Control {
                    x: firewallTitle3.x
                    implicitWidth: addrIp.width + addrSlash.width + addrSubnetMask.width
                    implicitHeight: addrIp.height
                    Component.onCompleted: firewallTitle3.implicitWidth = width

                    function fixMaskLength() {
                        const max = addrIp.valid == 4 ? 32 : 128;
                        const mask = parseInt(addrSubnetMask.text);
                        if (mask > max) {
                            model.mask = max;
                        } else if (model.mask != mask) {
                            model.mask = mask
                        }
                    }

                    TextField {
                        property int valid: 4
                        id: addrIp
                        width: defaultFont.width * 15
                        selectByMouse: true
                        text: model.addr
                        color: valid ? palette.text : "white"
                        onValidChanged: {
                            background.color = valid ? palette.base : "red";
                            parent.fixMaskLength();
                        }
                        onTextChanged: {
                            if (model.addr != text) model.addr = text;
                            valid = isIP(text) || text.length == 0;
                        }
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
                        validator: RegExpValidator { regExp: /[0-9]{0,3}/ }
                        horizontalAlignment: TextInput.AlignHCenter
                        text: model.mask
                        onTextChanged: parent.fixMaskLength()
                    }
                }
                Control {
                    id: portRange
                    x: firewallTitle4.x
                    implicitWidth: portRangeBegin.width + portHyphen.width + portRangeEnd.width
                    implicitHeight: portRangeBegin.height
                    Component.onCompleted: firewallTitle4.implicitWidth = width

                    function fixPortRange() {
                        if ((!model.portBegin && model.portEnd) || model.portBegin == model.portEnd) {
                            model.portBegin = model.portEnd;
                            model.portEnd = 0;
                        }
                    }

                    TextField {
                        id: portRangeBegin
                        width: font.pointSize * 5
                        validator: RegExpValidator { regExp: /[0-9]{0,5}/ }
                        selectByMouse: true
                        horizontalAlignment: TextInput.AlignHCenter
                        text: model.portBegin ? model.portBegin : ""
                        onTextChanged: {
                            let portB = parsePort(portRangeBegin.text);
                            let portE = parsePort(portRangeEnd.text);
                            if (model.portBegin != portB)
                                model.portBegin = portB

                            portRangeEnd.valid = !portE || portE >= portB;
                        }
                        onEditingFinished: parent.fixPortRange()
                    }
                    Label {
                        id: portHyphen
                        anchors.left: portRangeBegin.right
                        text: " - "
                        anchors.verticalCenter: parent.verticalCenter
                    }
                    TextField {
                        property bool valid: true
                        id: portRangeEnd
                        anchors.left: portHyphen.right
                        width: font.pointSize * 5
                        validator: RegExpValidator { regExp: /[0-9]{0,5}/ }
                        selectByMouse: true
                        horizontalAlignment: TextInput.AlignHCenter
                        text: model.portEnd ? model.portEnd : ""
                        color: valid ? palette.text : "white"
                        onValidChanged: background.color = valid ? palette.base : "red"
                        onTextChanged: {
                            let portB = parsePort(portRangeBegin.text);
                            let portE = parsePort(portRangeEnd.text);
                            if (model.portEnd != portE)
                                model.portEnd = portE

                            valid = !portE || portE >= portB;
                        }
                        onEditingFinished: parent.fixPortRange()
                    }
                }
                ComboBox {
                    x: firewallTitle5.x
                    currentIndex: target
                    onCurrentIndexChanged: if (target != currentIndex) target = currentIndex
                    model: defaultTarget.model
                    textRole: "name"
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

    RateLimitRulesPopup {
        id: rateLimitRules
    }

    RowLayout {
        id: tableFooter
        anchors.bottom: parent.bottom
        width: parent.width
        implicitHeight: applyBtn.height
        Label {
            text: qsTr("Default Target: ")
        }
        ComboBox {
            id: defaultTarget
            currentIndex: backend.default_target
            onCurrentIndexChanged: if (backend.default_target != currentIndex) backend.default_target = currentIndex
            model: ListModel {
                ListElement {
                    name: qsTr("Accept")
                }
                ListElement {
                    name: qsTr("Drop")
                }
            }
            textRole: "name"
            Component.onCompleted: {
                currentIndex = backend.default_target

                backend.rate_rules.modelReset.connect(() => {
                    const count = backend.rate_rules.rowCount()
                    for (var i = 0; i < count; i++) {
                        const index = backend.rate_rules.index(i, 0)
                        const name = backend.rate_rules.data(index, Qt.UserRole)
                        model.append({"name": name.toString()})
                    }
                })
                backend.rate_rules.dataChanged.connect((topLeft, bottomRight, roles) => {
                    console.assert(topLeft == bottomRight)
                    const name = backend.rate_rules.data(topLeft, Qt.UserRole)
                    model.setProperty(2 + topLeft.row, "name", name.toString())
                })
                backend.rate_rules.rowsRemoved.connect((_, first, last) => {
                    console.assert(first == last)
                    model.remove(2 + first)
                })
                backend.rate_rules.rowsInserted.connect((_, first, last) => {
                    console.assert(first, last, model.count - 1)
                    const index = backend.rate_rules.index(first, 0);
                    const name = backend.rate_rules.data(index, Qt.UserRole)
                    model.append({"name": name.toString()})
                })
            }
        }
        Item {
            Layout.fillWidth: true
        }
        Button {
            text: qsTr("Rate Limit Rules")
            onClicked: rateLimitRules.open()
        }
        Button {
            id: applyBtn
            text: qsTr("Apply")
            onClicked: if (backend.daemon_connected) {
                backend.apply_rules()
            } else if (backend.daemon_exists()) {
                backend.connect_to_daemon()
            } else {
                startDaemonPopup.open()
            }
            Component.onCompleted: {
                backend.apply_rules_error.connect((err) => {
                    errorPopup.message = qsTr("Illegal rules:")
                    errorPopup.error = err
                    errorPopup.open()
                })
            }
        }
    }
}
