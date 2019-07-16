import QtQuick 2.8
import QtQuick.Layouts 1.3
import QtQuick.Controls 2.3
import QtCharts 2.3

Item {
    Timer {
        interval: 1000
        repeat: true
        running: true
        onTriggered: {
            backend.refresh_monitor()
            refreshHistoryChart()
        }
    }
    function refreshHistoryChart() {
        const colors = ["209fdf", "99ca53", "f6a625", "6d5fd5", "bf593e"]
        history.removeAllSeries()
        const yValue = backend.charts.reduce((acc, v) => {
            const max = Math.max(...v.model.slice(Math.max(v.model.length - backend.chart_x_size, 0)))
            const [divisor, unit] = formatBytesRaw(max)
            return (max > acc.max) ? { div: divisor, unit: unit, max: max } : acc
        }, { max: -1 })

        if (yValue.max == -1) {
            return
        }

        const maxHistoryLength = Math.max(...backend.charts.map((v) => v.model.length))
        xAxis.max = maxHistoryLength - 1
        xAxis.min = xAxis.max - backend.chart_x_size

        yAxis.max = yValue.max / yValue.div
        yAxis.labelFormat = "%.0f " + yValue.unit + "/s"
        backend.charts.forEach((seriesData, i) => {
            const name = seriesData.name.split("/").pop()
            const series = history.createSeries(ChartView.SeriesTypeArea, name, xAxis, yAxis)
            series.pointsVisible = true
            series.color = "#AA" + colors[i]
            const xPadding = maxHistoryLength - seriesData.model.length
            seriesData.model.forEach((y, x) => {
                series.upperSeries.append(x + xPadding, y / yValue.div)
            })
        })
    }
    ChartView {
        id: history
        width: parent.width - traffic.width
        height: parent.height * 0.7
        title: "History"
        antialiasing: true

        ValueAxis {
            id: xAxis
            tickCount: 12
            labelFormat: "x"
        }
        ValueAxis {
            id: yAxis
        }

        Component.onCompleted: refreshHistoryChart()
    }

    Frame {
        id: logs
        width: parent.width - traffic.width
        height: parent.height * 0.3
        anchors.top: history.bottom
        topPadding: 0

        ColumnLayout {
            anchors.fill: parent

            RowLayout {
                Layout.fillWidth: true
                height: separator.implicitHeight
                spacing: 0

                Pane {
                    id: logsTitle0
                    implicitWidth: 40
                    padding: 0
                    Label {
                        text: "R"
                        font.bold: true
                        anchors.horizontalCenter: parent.horizontalCenter
                    }
                }
                ToolSeparator {
                    id: separator
                }
                Pane {
                    id: logsTitle1
                    topPadding: 0
                    bottomPadding: 0
                    Layout.fillWidth: true
                    Label {
                        text: "Program"
                        font.bold: true
                        anchors.horizontalCenter: parent.horizontalCenter
                    }
                }
                ToolSeparator {}
                Pane {
                    id: logsTitle2
                    implicitWidth: defaultFont.width * 2
                    padding: 0
                    Label {
                        text: "↹"
                        font.bold: true
                        anchors.horizontalCenter: parent.horizontalCenter
                    }
                }
                ToolSeparator {}
                Pane {
                    id: logsTitle3
                    implicitWidth: defaultFont.width * 15
                    padding: 0
                    Label {
                        text: "Address"
                        font.bold: true
                        anchors.horizontalCenter: parent.horizontalCenter
                    }
                }
                ToolSeparator {}
                Pane {
                    id: logsTitle4
                    implicitWidth: defaultFont.width * 7
                    padding: 0
                    Label {
                        text: "Protocol"
                        font.bold: true
                        anchors.horizontalCenter: parent.horizontalCenter
                    }
                }
                ToolSeparator {}
                Pane {
                    id: logsTitle5
                    implicitWidth: defaultFont.width * 10
                    padding: 0
                    Label {
                        text: "Size"
                        font.bold: true
                        anchors.horizontalCenter: parent.horizontalCenter
                    }
                }
                ToolSeparator {}
                Pane {
                    id: logsTitle6
                    implicitWidth: defaultFont.width * 4
                    padding: 0
                    Label {
                        text: "Rule"
                        font.bold: true
                        anchors.horizontalCenter: parent.horizontalCenter
                    }
                }
            }

            ListView {
                clip: true
                Layout.fillHeight: true
                Layout.fillWidth: true
                model: backend.logs

                delegate: Item {
                    width: parent.width
                    height: logStatus.height

                    Rectangle {
                        id: logStatus
                        width: 40
                        height: 40
                        color: if (model.dropped) { "red" } else { "green" }
                    }
                    Label {
                        x: logsTitle1.x
                        width: logsTitle1.width
                        clip: true
                        text: model.exe
                        anchors.verticalCenter: parent.verticalCenter
                    }
                    Label {
                        x: logsTitle2.x + (logsTitle2.width - width) / 2
                        text: (model.input ?  "⇤" : "↦")
                        font.pointSize: defaultFont.font.pointSize * 1.5
                        anchors.verticalCenter: parent.verticalCenter
                    }
                    Label {
                        x: logsTitle3.x
                        width: logsTitle3.width
                        clip: true
                        text: model.addr
                        anchors.verticalCenter: parent.verticalCenter
                    }
                    Label {
                        x: logsTitle4.x + (logsTitle4.width - width) / 2
                        text: model.protocol
                        anchors.verticalCenter: parent.verticalCenter
                    }
                    Label {
                        x: logsTitle5.x + logsTitle5.width - width
                        text: formatBytes(model.len)
                        anchors.verticalCenter: parent.verticalCenter
                    }
                    Label {
                        x: logsTitle6.x
                        text: model.matched_rule != 0 ? model.matched_rule : qsTr("Default Rule")
                        anchors.verticalCenter: parent.verticalCenter
                    }
                }
            }
        }
    }

    Frame {
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
                    id: trafficTitle0
                    topPadding: 0
                    bottomPadding: 0
                    Layout.fillWidth: true
                    Label {
                        text: "Program"
                        font.bold: true
                        anchors.horizontalCenter: parent.horizontalCenter
                    }
                }
                ToolSeparator {}
                Pane {
                    id: trafficTitle1
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
                    id: trafficTitle2
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
                model: backend.traffic
                delegate: Item {
                    width: parent.width
                    height: separator.implicitHeight

                    Label {
                        clip: true
                        width: trafficTitle0.width
                        text: model.exe.split("/").pop()
                    }
                    Label {
                        x: trafficTitle1.x + trafficTitle1.width - width
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
