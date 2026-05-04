from django.db.models.signals import pre_save, post_save
from django.dispatch import receiver
from apps.audit.models import AuditLog
from apps.payments.models import Payment
from apps.reservations.models import Reservation
from apps.stays.models import Stay
from apps.rooms.models import Room

def _log(instance, action, before=None):
    AuditLog.objects.create(user=None,action=action,entity_type=instance.__class__.__name__,entity_id=str(instance.pk or ''),before=before or {},after={})

@receiver(post_save, sender=Payment)
def payment_log(sender, instance, created, **kwargs): _log(instance, 'payment_created' if created else 'payment_updated')
@receiver(post_save, sender=Reservation)
def reservation_log(sender, instance, created, **kwargs): _log(instance, 'reservation_created' if created else 'reservation_updated')
@receiver(post_save, sender=Stay)
def stay_log(sender, instance, created, **kwargs): _log(instance, 'checkin_checkout')
@receiver(pre_save, sender=Room)
def room_status_log(sender, instance, **kwargs):
    if instance.pk:
        prev=Room.objects.get(pk=instance.pk)
        if prev.status!=instance.status: _log(instance,'room_status_changed',before={'status':prev.status})
