#!/usr/bin/env python3
"""Launch only the task-owned Ubuntu disk, read-only input medium and output."""
import json,os,socket,subprocess
from pathlib import Path
stage=Path(__file__).resolve().parent;root=stage.parent
with socket.socket() as sock:sock.bind(('127.0.0.1',0));port=sock.getsockname()[1]
command=[str(root/'tools/vfkit-0.6.4'),'--cpus','2','--memory','4096','--bootloader',f'efi,variable-store={stage}/ubuntu-efi-vars'+(',create' if not (stage/'ubuntu-efi-vars').exists() else ''),
 '--device',f'virtio-blk,path={stage}/ubuntu-build-16g.raw','--device',f'virtio-blk,path={stage}/clmm-inputs-readonly.iso,readonly',
 '--device',f'virtio-fs,sharedDir={stage}/output,mountTag=r01out','--device','virtio-serial,stdio','--device','virtio-rng',
 '--device','virtio-net,nat,mac=52:54:00:72:01:52','--cloud-init',','.join(str(stage/'cloud-init'/n) for n in ['user-data','meta-data','network-config']),
 '--pidfile',str(stage/'ubuntu-vfkit.pid'),'--restful-uri',f'tcp://127.0.0.1:{port}']
env=os.environ.copy();env['TMPDIR']=str(root/'tmp');env['LC_ALL']='C';env.pop('SSH_AUTH_SOCK',None)
record={'command':command,'host_env_overrides':{'TMPDIR':env['TMPDIR'],'LC_ALL':'C'},'SSH_AUTH_SOCK_removed':True,'cpus':2,'memory_MiB':4096,'writable_guest_disk_bytes':16<<30,'readonly_input_iso_bytes':(stage/'clmm-inputs-readonly.iso').stat().st_size,'all_virtual_block_devices_bytes':(16<<30)+(stage/'clmm-inputs-readonly.iso').stat().st_size,'output_shared_directory':str(stage/'output'),'source_vendor_tools_host_directory_shares':[],'task_own_rest_endpoint':f'http://127.0.0.1:{port}','native_Apple_NAT_only':True,'host_uid':os.geteuid()}
(stage/'ubuntu-launch.json').write_text(json.dumps(record,indent=2)+'\n')
result=subprocess.run(['/usr/bin/script','-q',str(stage/'ubuntu-console.typescript'),*command],env=env,cwd=stage)
(stage/'ubuntu-launch-exit.json').write_text(json.dumps({'exit_code':result.returncode},indent=2)+'\n');raise SystemExit(result.returncode)
