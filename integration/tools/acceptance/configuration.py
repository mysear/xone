"""Temporary test configurations and explicit offline debug process selection."""
from pathlib import Path
import json
ROOT=Path(__file__).resolve().parents[3]
def config_for(name,directory,token,port):
 config=json.loads((ROOT/name/'config/config-test.yaml').read_text())
 config['api'].update(listen=f'127.0.0.1:{port}',auth_token=token,requests_per_minute=1000)
 config['state']['path']=str(directory/'state')
 config['logging']['level']='info'
 return config

def save_config(directory,config):
 directory.mkdir(parents=True,exist_ok=True)
 if config['service']['name']=='dex-router' and config['api'].get('auth_token'):
  config['journal'].setdefault('source_auth_tokens',{url:config['api']['auth_token'] for url in config['journal']['source_urls']})
 path=directory/'config-test.yaml';path.write_text(json.dumps(config,indent=2)+'\n');path.chmod(0o600)
 return path

def sample_for(chain,directory,timestamp_ms):
 source=ROOT/'integration/test'/chain.split(':')[0]/'baseline.json'
 sample=json.loads(source.read_text());sample['reference']['block']['timestamp_ms']=timestamp_ms
 directory.mkdir(parents=True,exist_ok=True)
 (directory/(chain.replace(':','-')+'.json')).write_text(json.dumps(sample))

def service_command(name,config,fixtures=None):
 if fixtures is not None and name!='dex-router':
  binary=ROOT/name/('bin/fixture-parser-go' if name=='chain-parser-go' else 'target/debug/fixture-parser-rust')
  return [str(binary),'--config',str(config),'--fixtures',str(fixtures)]
 binary=ROOT/name/('bin/chain-parser-go' if name=='chain-parser-go' else 'target/debug/'+name)
 return [str(binary),'--config',str(config)]
