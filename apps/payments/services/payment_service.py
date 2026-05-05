from django.db import transaction
from apps.payments.models import Payment
from apps.payments.adapters.gateway import MockPOSAdapter
class PaymentService:
    @staticmethod
    @transaction.atomic
    def create_payment(**data):
        adapter=MockPOSAdapter(); tx=adapter.send_payment(data['amount'])
        p=Payment.objects.create(**data,status='PENDING',rrn=tx['reference'])
        return p
