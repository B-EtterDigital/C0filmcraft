// Original C0vid browser-codec adapter, MIT OR Apache-2.0.
// Codecs are provided by the browser. No software H.264/HEVC/AAC implementation is shipped.
const MAX_QUEUE = 32;
const MAX_FLUSH_MS = 30000;
const MAX_VIDEO_BYTES = 256 * 1024 * 1024;
const MAX_AUDIO_FRAMES = 1 << 20;
const MAX_CHANNELS = 32;
const MAX_PCM_BYTES = 64 * 1024 * 1024;

async function supported(type, config) {
  try { return !!(await globalThis[type]?.isConfigSupported(config))?.supported; }
  catch { return false; }
}
export async function probeCodecs() {
  return {
    video: await supported('VideoEncoder', {codec:'avc1.42001f', width:640, height:480, bitrate:2000000, framerate:30, avc:{format:'avc'}}),
    audio: await supported('AudioEncoder', {codec:'mp4a.40.2', sampleRate:48000, numberOfChannels:2, bitrate:128000}),
    audioDecode: await supported('AudioDecoder', {codec:'mp4a.40.2', sampleRate:48000, numberOfChannels:2, description:new Uint8Array([0x11,0x90])})
  };
}
function state() { return {packets:[], description:null, error:null, flushing:false, done:false, firstTimestamp:null}; }
function output(s, chunk, metadata) {
  if (s.firstTimestamp === null) s.firstTimestamp = chunk.timestamp;
  if (metadata?.decoderConfig?.description) {
    const description=metadata.decoderConfig.description;
    s.description=ArrayBuffer.isView(description)
      ? new Uint8Array(description.buffer,description.byteOffset,description.byteLength).slice()
      : new Uint8Array(description).slice();
  }
  const bytes = new Uint8Array(chunk.byteLength);
  chunk.copyTo(bytes);
  s.packets.push({bytes, key:chunk.type === 'key', duration:chunk.duration || 0});
}
export function createVideo(width, height, num, den, bitrate, options={}) {
  const {codec='avc1.42001f',keyInterval=0,bitrateMode='variable'}=options;
  if (!Number.isSafeInteger(width) || !Number.isSafeInteger(height) || width < 1 || height < 1 || width*height*4 > MAX_VIDEO_BYTES) throw new Error('Video frame exceeds browser allocation limit');
  const s = state();
  s.width = width; s.height = height; s.num = num; s.den = den; s.keyInterval=keyInterval || Math.max(1,Math.round(num/den)*2);
  s.encoder = new VideoEncoder({output:(chunk, metadata) => output(s, chunk, metadata), error:error => {s.error=String(error);}});
  try { s.encoder.configure({codec, width, height, bitrate, bitrateMode, framerate:num/den, latencyMode:'realtime', avc:{format:'avc'}}); }
  catch (error) { s.encoder.close(); throw error; }
  return s;
}
export function encodeVideo(s, bytes, index) {
  const duration = Math.round(1e6 * s.den / s.num);
  const frame = new VideoFrame(bytes, {format:'RGBA', codedWidth:s.width, codedHeight:s.height, timestamp:Math.round(index*1e6*s.den/s.num), duration});
  try { s.encoder.encode(frame, {keyFrame:index === 0 || index % s.keyInterval === 0}); }
  finally { frame.close(); }
}
export function createAudio(rate, channels, bitrate) {
  const s=state(); s.rate=rate; s.channels=channels; s.samples=0;
  s.encoder = new AudioEncoder({output:(chunk,metadata) => output(s,chunk,metadata), error:error => {s.error=String(error);}});
  try {s.encoder.configure({codec:'mp4a.40.2',sampleRate:rate,numberOfChannels:channels,bitrate});}
  catch (error) {s.encoder.close(); throw error;}
  return s;
}
export function encodeAudio(s, planar, frames) {
  if (frames > MAX_AUDIO_FRAMES || s.channels > MAX_CHANNELS) throw new Error('Audio encode block exceeds browser limits');
  const data=new AudioData({format:'f32-planar',sampleRate:s.rate,numberOfFrames:frames,numberOfChannels:s.channels,timestamp:Math.round(s.samples*1e6/s.rate),data:planar});
  try {s.encoder.encode(data); s.samples += frames;}
  finally {data.close();}
}
export function takePackets(s) {const packets=s.packets; s.packets=[]; return packets;}
export function encoderReady(s) {
  if (s.error) throw new Error(s.error);
  return s.encoder.encodeQueueSize < MAX_QUEUE;
}
export function encoderFinishReady(s) {
  if (s.error) throw new Error(s.error);
  if (!s.flushing) {
    s.flushing=true; s.deadline=performance.now()+MAX_FLUSH_MS;
    s.encoder.flush().then(() => {s.done=true;},error => {s.error=String(error);});
  }
  if (!s.done && performance.now()>s.deadline) throw new Error('Browser encoder flush timed out');
  return s.done;
}
export function closeEncoder(s) {if (s.encoder.state !== 'closed') s.encoder.close();}

// Bounded random-access AAC windows. Decoder timestamps are presentation timestamps, including
// the MP4 edit offset supplied by Rust; priming before media time zero is naturally excluded.
export function createAudioDecoder(description,rate,channels) {
  if (!rate || channels < 1 || channels > MAX_CHANNELS) throw new Error('Unsupported AAC channel layout');
  const s={rate,channels,blocks:[],error:null,done:false,flushing:false,bytes:0};
  s.decoder=new AudioDecoder({output:data => {
    try {
      if (data.numberOfChannels !== s.channels) throw new Error('AAC channel layout changed during decoding');
      const frames=data.numberOfFrames;
      const bytes=frames*data.numberOfChannels*4;
      if (s.bytes+bytes > MAX_PCM_BYTES) throw new Error('AAC decode window exceeds 64 MiB');
      const channels=[];
      for(let plane=0;plane<data.numberOfChannels;plane++) {
        const pcm=new Float32Array(frames);
        data.copyTo(pcm,{planeIndex:plane,format:'f32-planar'});
        channels.push(pcm);
      }
      s.bytes+=bytes;
      s.blocks.push({start:Math.round(data.timestamp*data.sampleRate/1e6),rate:data.sampleRate,channels,frames});
    } catch(error) {s.error=String(error);}
    finally {data.close();}
    },error:error=>{s.error=String(error);}});
  try {s.decoder.configure({codec:'mp4a.40.2',sampleRate:rate,numberOfChannels:channels,description});}
  catch(error) {s.decoder.close();throw error;}
  return s;
}
export function decodeAudio(s,bytes,timestamp,duration) {s.decoder.decode(new EncodedAudioChunk({type:'key',data:bytes,timestamp,duration}));}
export function flushAudioDecoder(s) {
  if (!s.flushing) {s.flushing=true;s.deadline=performance.now()+MAX_FLUSH_MS;s.decoder.flush().then(()=>{s.done=true;},error=>{s.error=String(error);});}
}
export function readAudio(s,start,frames,rate) {
  if (s.error) throw new Error('AAC WebCodecs: '+s.error);
  if (!s.done) {if(performance.now()>s.deadline)throw new Error('AAC browser decoder flush timed out');return null;}
  if (!s.blocks.length && frames) throw new Error('AAC decoder returned no samples');
  if (frames > MAX_AUDIO_FRAMES || frames*s.channels*4 > MAX_PCM_BYTES || !rate) throw new Error('Audio request exceeds browser limits');
  const output=new Float32Array(frames*s.channels);
  for (const [blockIndex,block] of s.blocks.entries()) {
    const next=s.blocks[blockIndex+1];
    const first=Math.max(0,Math.ceil(block.start*rate/block.rate-start));
    const last=Math.min(frames,Math.ceil((block.start+block.frames)*rate/block.rate-start));
    for(let i=first;i<last;i++) {
      const position=(start+i)*block.rate/rate-block.start;
      const at=Math.floor(position), fraction=position-at;
      if (at < 0 || at >= block.frames) continue;
      for(let c=0;c<s.channels;c++) {
        const a=block.channels[c]?.[at] || 0;
        const b=block.channels[c]?.[at+1] ?? (next?.rate===block.rate && next.start===block.start+block.frames ? next.channels[c]?.[0] : a) ?? a;
        const sample=a+(b-a)*fraction;
        output[c*frames+i]=Number.isFinite(sample)?sample:0;
      }
    }
  }
  return output;
}
export function closeAudioDecoder(s) {if(s.decoder.state!=='closed')s.decoder.close();}
