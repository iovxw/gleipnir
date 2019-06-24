import QtQuick 2.8
import QtQuick.Layouts 1.3
import QtQuick.Controls 2.3
import QtQml.Models 2.1

Popup {
    property real realY: Math.round((parent.height - height) / 2)
    parent: Overlay.overlay
    x: Math.round((parent.width - width) / 2)
    y: realY
    width: root.width * 0.5
    height: root.height * 0.8
    enter: Transition {
        NumberAnimation {
            property: "y"
            easing.type: Easing.OutBack
            from: 0
            to: rateLimitRules.realY
            duration: 200
        }
    }
    exit: Transition {
        NumberAnimation {
            property: "y"
            easing.type: Easing.InBack
            from: rateLimitRules.realY
            to: 0
            duration: errorPopup.visible ? 0 : 200
        }
    }
    RowLayout {
        id: rateLimitRulesTitle
        width: parent.width
        height: separator.implicitHeight
        spacing: 0

        Pane {
            id: rateLimitRulesTitle0
            Layout.fillWidth: true
            topPadding: 0
            bottomPadding: 0
            Label {
                text: qsTr("Name")
                font.bold: true
                anchors.horizontalCenter: parent.horizontalCenter
            }
        }
        ToolSeparator {
            id: separator
        }
        Pane {
            id: rateLimitRulesTitle1
            topPadding: 0
            bottomPadding: 0
            implicitWidth: defaultFont.width * 10
            Label {
                text: qsTr("Limit")
                font.bold: true
                anchors.horizontalCenter: parent.horizontalCenter
            }
        }
        ToolSeparator {}
        Pane {
            id: rateLimitRulesTitle2
        }
    }
    ListView {
        width: parent.width
        anchors.top: rateLimitRulesTitle.bottom
        anchors.bottom: parent.bottom
        clip: true
        model: backend.rate_rules
        delegate: Pane {
            implicitHeight: rateLimitRuleName.height + topPadding + bottomPadding
            padding: 0
            topPadding: separator.padding
            bottomPadding: topPadding
            TextField {
                id: rateLimitRuleName
                x: rateLimitRulesTitle0.x
                width: rateLimitRulesTitle0.width
                text: model.name
                onTextChanged: if (model.name != text) model.name = text
            }
            TextField {
                x: rateLimitRulesTitle1.x
                width: rateLimitRulesTitle1.width
                text: model.limit
                onTextChanged: if (model.limit != text) model.limit = parseInt(text)
            }
            Button {
                x: rateLimitRulesTitle2.x
                text: "×"
                width: height
                onClicked: backend.remove_rate_rule(index)
                Component.onCompleted: rateLimitRulesTitle2.implicitWidth = width
            }
        }
        footer: Pane {
            width: parent.width
            padding: 0
            topPadding: separator.padding
            bottomPadding: topPadding

            Button {
                id: rateLimitRulesAddBtn
                width: parent.width
                text: "+"
                onClicked: backend.new_rate_rule()
            }
        }
    }
}
