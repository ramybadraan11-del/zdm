//! ZDM About Dialog
//! Developed by Ramy Badraan

import React, { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Info, Github, Cpu, HardDrive, Monitor } from 'lucide-react';
import { Modal } from './ui';
import { Logo } from './Logo';
import { useI18n } from '../i18n';
import { AppInfo } from '../types';

interface Props {
  open: boolean;
  onClose: () => void;
}

/** Open the project forum in the user's browser. */
async function openForum(): Promise<void> {
  const url = 'https://forum.zyzoom.net/';
  try {
    const { open } = await import('@tauri-apps/plugin-shell');
    await open(url);
  } catch {
    // Fall back to the webview when the shell plugin is unavailable.
    window.open(url, '_blank', 'noopener,noreferrer');
  }
}

export const AboutModal: React.FC<Props> = ({ open, onClose }) => {
  const { t } = useI18n();
  const [info, setInfo] = useState<AppInfo | null>(null);

  useEffect(() => {
    if (!open) return;
    invoke<AppInfo>('get_app_info')
      .then(setInfo)
      .catch(() => setInfo(null));
  }, [open]);

  return (
    <Modal
      open={open}
      onClose={onClose}
      title={t('aboutTitle')}
      icon={<Info size={17} />}
      width="sm"
      footer={
        <button type="button" onClick={onClose} className="btn-primary h-9 px-4 text-[12.5px]">
          {t('closeAbout')}
        </button>
      }
    >
      <div className="text-center">
        <div className="mx-auto w-fit rounded-3xl overflow-hidden ring-1 ring-lineStrong/60 shadow-panel mb-3">
          <Logo size={72} />
        </div>

        <h3 className="text-[15px] font-extrabold brand-text">Zyzoom Download Manager</h3>
        <p className="text-[13px] text-inkMuted mt-0.5">
          {t('version')} {info?.version ?? '1.0.0'} · {info?.architecture ?? '—'}
        </p>

        <p className="text-[13.5px] text-inkSoft leading-relaxed mt-3 max-w-sm mx-auto">
          {t('aboutBody')}
        </p>

        <div className="mt-4 space-y-1.5 text-start">
          <Fact icon={<Cpu size={12} />} label={t('engineLabel')} value={info?.engine ?? '—'} />
          <Fact
            icon={<Monitor size={12} />}
            label={t('desktopStatus')}
            value={`${info?.os ?? '—'} · ${info?.architecture ?? '—'}`}
          />
          <Fact
            icon={<HardDrive size={12} />}
            label={t('dataDir')}
            value={info?.data_dir ?? '—'}
            ltr
          />
        </div>

        <div className="mt-4 pt-3 border-t border-line flex items-center justify-center gap-1.5 text-[13px] text-inkMuted">
          <Github size={12} />
          <span>
            {t('developedBy')}{' '}
            {/* The developer's name opens the project forum: the natural place to
                ask about ZDM, and the credits line is where people look for it. */}
            <button
              type="button"
              onClick={() => void openForum()}
              className="about-credit"
              title="forum.zyzoom.net"
            >
              Ramy Badraan
            </button>
          </span>
        </div>
      </div>
    </Modal>
  );
};

const Fact: React.FC<{ icon: React.ReactNode; label: string; value: string; ltr?: boolean }> = ({
  icon,
  label,
  value,
  ltr,
}) => (
  <div className="flex items-start gap-2 px-2.5 py-1.5 rounded-lg bg-panelHover/40">
    <span className="text-inkMuted mt-0.5 shrink-0">{icon}</span>
    <span className="text-[12.5px] text-inkMuted shrink-0">{label}</span>
    <span
      className={`text-[12.5px] text-inkSoft truncate ms-auto text-end ${ltr ? 'always-ltr mono' : ''}`}
      dir={ltr ? 'ltr' : undefined}
      title={value}
    >
      {value}
    </span>
  </div>
);
