#!/usr/bin/env bash
# Synthesize Chinese narration via Polly neural Zhiyu, then concat with ffmpeg.
# Targets per-segment durations; if a segment overshoots, we lean on the SRT
# having enough slack (pauses between segments absorb small drift).
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEMO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
OUT_DIR="$DEMO_ROOT/narration"
SRT_FILE="$DEMO_ROOT/subtitles/narration.srt"
REGION="${AWS_REGION:-us-east-1}"

mkdir -p "$OUT_DIR" "$(dirname "$SRT_FILE")"

# Define segments: index|start_s|text  (text is SSML — wrap English tech terms
# with <prosody rate="85%"> + surrounding <break time="..."/> for clarity).
segments=(
  "1|0|<speak>凌晨三点，<prosody rate=\"90%\">Grafana</prosody> 告警响起。订单服务 <lang xml:lang=\"en-US\"><say-as interpret-as=\"characters\">RCA</say-as>-<prosody rate=\"85%\">demo</prosody></lang> 的错误率飙到百分之六。在传统模式下，值班工程师要打开六个监控页面、翻三份运行手册，才能定位问题。<break time=\"250ms\"/><prosody rate=\"90%\">OpsAgent</prosody>，换个打法。</speak>"
  "2|19|<speak>一个 Agent、三栏视图：左边是思考时间线，中间是根因报告，右边是证据摘录。<break time=\"200ms\"/>它并行调用 <lang xml:lang=\"en-US\"><prosody rate=\"85%\">kubectl</prosody></lang><break time=\"200ms\"/>、<prosody rate=\"80%\">Mimir</prosody><break time=\"200ms\"/>、<prosody rate=\"80%\">Loki</prosody><break time=\"200ms\"/>、<prosody rate=\"80%\">Runbook</prosody> 知识库，不是聊天，是查案。</speak>"
  "3|40|<speak>证据一条条摆出来：<lang xml:lang=\"en-US\"><prosody rate=\"85%\">kubectl</prosody></lang> 发现金丝雀 <prosody rate=\"85%\">Pod</prosody> 刚上线十分钟，内存持续增长，已被 <lang xml:lang=\"en-US\"><say-as interpret-as=\"characters\">OOM</say-as> <prosody rate=\"85%\">Killed</prosody></lang> 三次。<break time=\"200ms\"/><prosody rate=\"80%\">Mimir</prosody> 指标确认错误率源头，<break time=\"200ms\"/><prosody rate=\"80%\">Loki</prosody> 日志抓到内存泄漏现场。<break time=\"200ms\"/><lang xml:lang=\"en-US\"><prosody rate=\"80%\">GraphRAG</prosody></lang> 同步翻出 <lang xml:lang=\"en-US\"><say-as interpret-as=\"characters\">RCA</say-as>-<prosody rate=\"85%\">demo</prosody></lang><break time=\"250ms\"/> <prosody rate=\"90%\">运维手册</prosody>里的<break time=\"150ms\"/>金丝雀排查章节。</speak>"
  "4|65|<speak>证据链闭环：根因锁定 <prosody rate=\"85%\">v2-buggy</prosody> 版本的 <prosody rate=\"80%\">BUGGY</prosody> 环境变量触发内存泄漏。<break time=\"200ms\"/>Runbook 第四点二节明确给出处置方案：立即 abort 金丝雀、回退到稳定版本 v1。<break time=\"200ms\"/>Agent 给出建议，决策权在人。</speak>"
  "5|88|<speak>切到 Deployments 页面，按 Agent 建议点下回滚。<break time=\"200ms\"/>卡片顶部进度条亮起，<prosody rate=\"80%\">Argo Rollouts</prosody> 被调用，abort 命令下发。<break time=\"200ms\"/>金丝雀流量从百分之二十切回零，<prosody rate=\"85%\">Pod</prosody> 实时终结。</speak>"
  "6|108|<speak>错误率应声回落。从告警到修复，两分钟。<prosody rate=\"90%\">OpsAgent</prosody>，让每一位工程师都多一个 <prosody rate=\"80%\">SRE</prosody> 搭档。</speak>"
)

seg_count="${#segments[@]}"
total_target=120

echo "[polly] Synthesizing ${seg_count} segments..."

for entry in "${segments[@]}"; do
  IFS='|' read -r idx start text <<< "$entry"
  mp3="$OUT_DIR/seg${idx}.mp3"
  echo "  [${idx}] start=${start}s text='${text:0:40}…'"
  aws polly synthesize-speech \
    --region "$REGION" \
    --engine neural \
    --language-code cmn-CN \
    --voice-id Zhiyu \
    --output-format mp3 \
    --text-type ssml \
    --text "$text" \
    "$mp3" >/dev/null
done

# Build concat list with silence padding between segments.
# Each segment gets padded with silence so it fills its allocated slot.
concat_list="$OUT_DIR/.concat.txt"
: > "$concat_list"

prev_end=0
for i in "${!segments[@]}"; do
  entry="${segments[$i]}"
  IFS='|' read -r idx start text <<< "$entry"

  # Pad with silence up to segment start (if any gap).
  if (( start > prev_end )); then
    gap=$((start - prev_end))
    silence="$OUT_DIR/.silence-${idx}-pre.mp3"
    ffmpeg -y -f lavfi -i "anullsrc=r=24000:cl=mono" -t "$gap" -q:a 9 -acodec libmp3lame "$silence" >/dev/null 2>&1
    echo "file '$silence'" >> "$concat_list"
  fi

  # Actual narration.
  echo "file '$OUT_DIR/seg${idx}.mp3'" >> "$concat_list"

  # Read duration.
  dur=$(ffprobe -v error -show_entries format=duration -of csv=p=0 "$OUT_DIR/seg${idx}.mp3")
  dur_int=$(printf '%.0f' "$dur")
  prev_end=$((start + dur_int))

  echo "  [${idx}] duration=${dur}s → end ≈ ${prev_end}s"
done

# Final silence padding to 120s total.
if (( prev_end < total_target )); then
  gap=$((total_target - prev_end))
  silence="$OUT_DIR/.silence-end.mp3"
  ffmpeg -y -f lavfi -i "anullsrc=r=24000:cl=mono" -t "$gap" -q:a 9 -acodec libmp3lame "$silence" >/dev/null 2>&1
  echo "file '$silence'" >> "$concat_list"
fi

# Concatenate into narration.mp3
final_mp3="$OUT_DIR/narration.mp3"
ffmpeg -y -f concat -safe 0 -i "$concat_list" -c copy "$final_mp3" 2>/dev/null
echo "[polly] wrote $final_mp3"

# Generate SRT with exact durations per segment.
echo "[polly] building SRT → $SRT_FILE"
python3 - <<'PY' > "$SRT_FILE"
import subprocess, os, textwrap

DEMO = os.environ.get("DEMO_ROOT", os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
OUT = f"{DEMO}/narration"

segments = [
    (1, 0,   "凌晨三点，Grafana 告警响起。订单服务 rca-demo 的错误率飙到百分之六。在传统模式下，值班工程师要打开六个监控页面、翻三份运行手册，才能定位问题。OpsAgent，换个打法。"),
    (2, 18,  "OpsAgent 自动启动根因分析智能体。右侧工具清单实时显示：Shell、Mimir、Loki、知识库。它并行调用，不是聊天。它不猜，它查。"),
    (3, 40,  "工具一个个亮起：kubectl 找到一个刚上线十分钟的金丝雀 Pod，内存持续增长，已被 OOMKilled 三次。Mimir 的指标确认错误率来源，Loki 的日志抓到内存泄漏现场。GraphRAG 同步翻出 rca-demo 手册里的金丝雀排查章节。"),
    (4, 65,  "交叉比对指标、日志、集群状态和运维手册，Agent 输出结构化根因报告。关键证据自动高亮：金丝雀版本、BUGGY 环境变量、内存泄漏。每一条结论都有来源。"),
    (5, 88,  "一键回滚。卡片顶部进度条亮起，Agent 调用 Argo Rollouts，把金丝雀流量切回稳定版本。金丝雀 Pod 实时终结。"),
    (6, 108, "错误率应声回落。从告警到修复，两分钟。OpsAgent，让每一位工程师都多一个 SRE 搭档。"),
]

def fmt(t):
    h = int(t // 3600); m = int(t % 3600 // 60); s = int(t % 60); ms = int((t - int(t)) * 1000)
    return f"{h:02d}:{m:02d}:{s:02d},{ms:03d}"

# Wrap long sentences to 22 chars per line for readability.
def wrap(text):
    return "\n".join(textwrap.wrap(text, width=22)) if len(text) > 22 else text

for (idx, start, text) in segments:
    mp3 = f"{OUT}/seg{idx}.mp3"
    dur = float(subprocess.check_output(
        ["ffprobe", "-v", "error", "-show_entries", "format=duration",
         "-of", "csv=p=0", mp3]).strip())
    end = start + dur
    print(f"{idx}\n{fmt(start)} --> {fmt(end)}\n{wrap(text)}\n")
PY
DEMO_ROOT="$DEMO_ROOT" : # variable available for the python subshell above — already exported via environment

echo "[polly] SRT ready: $SRT_FILE"
ls -la "$final_mp3" "$SRT_FILE"
